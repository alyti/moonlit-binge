use encre_css::{
    error::{Error, Result},
    generate, Config as EncreConfig,
};
use serde::Deserialize;
use std::{
    fs,
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::exit,
};
use wax::Glob;

#[derive(Default, PartialEq, Debug, Deserialize)]
struct Config {
    /// Specify which files should be scanned using globs.
    #[serde(default)]
    input: Vec<PathBuf>,
    #[serde(flatten)]
    encre_config: EncreConfig,
}
impl Config {
    fn from_file<T: AsRef<Path>>(path: T) -> Result<Self> {
        Ok(toml::from_str(&fs::read_to_string(&path).map_err(
            |e| Error::ConfigFileNotFound(path.as_ref().to_path_buf(), e),
        )?)?)
    }
}

fn gen_css<'a, T: AsRef<Path>>(
    sources: impl IntoIterator<Item = &'a str>,
    config: &EncreConfig,
    output: Option<T>,
) {
    let css = generate(sources, config);
    if let Some(file) = output {
        if let Some(parent) = file.as_ref().parent() {
            // Create parent directories
            fs::create_dir_all(parent).expect("failed to create parent directories");
        }
        fs::write(file, css).expect("failed to write to the file");
    } else {
        // If no file is specified, the CSS generated is written to the standard output
        println!("{css}");
    }
}

fn scan_path<T: AsRef<Path>>(glob_path: T, buffer: &mut String) {
    let (prefix, glob) = match Glob::new(
        glob_path
            .as_ref()
            .to_str()
            .expect("failed to convert the glob to a string"),
    ) {
        Ok(g) => g.partition(),
        Err(e) => panic!("{}", e),
    };

    if prefix == glob_path.as_ref() && prefix.is_file() {
        match fs::File::open(&glob_path) {
            Ok(mut file) => {
                let file_len = file
                    .seek(SeekFrom::End(0))
                    .expect("failed to seek to the end of the file");
                file.rewind()
                    .expect("failed to seek to the start of the file");

                #[allow(clippy::cast_possible_truncation)]
                buffer.reserve(file_len as usize);

                if let Err(e) = file.read_to_string(buffer) {
                    eprintln!(
                        "Failed to read the file {}: {}",
                        glob_path.as_ref().display(),
                        e
                    );
                }
            }
            Err(e) => eprintln!(
                "Failed to open the file {}: {}",
                glob_path.as_ref().display(),
                e
            ),
        }
    } else {
        glob.walk(prefix).for_each(|entry| {
            if let Ok(entry) = entry {
                let path = entry.path();

                if path.is_file() {
                    match fs::File::open(path) {
                        Ok(file) => {
                            let mut reader = BufReader::new(file);
                            let file_len = reader
                                .seek(SeekFrom::End(0))
                                .expect("failed to seek to the end of the file");
                            reader
                                .rewind()
                                .expect("failed to seek to the start of the file");

                            #[allow(clippy::cast_possible_truncation)]
                            buffer.reserve(file_len as usize);

                            if let Err(e) = reader.read_to_string(buffer) {
                                eprintln!(
                                    "Failed to read the file {}: {}",
                                    glob_path.as_ref().display(),
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Failed to open the file {}: {}",
                                glob_path.as_ref().display(),
                                e
                            );
                        }
                    }
                }
            }
        });
    }
}

fn build_single<T: AsRef<Path>>(config_file: &str, extra_input: Option<T>, output: Option<String>) {
    let mut config = match Config::from_file(config_file) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            Config::default()
        }
    };

    encre_css_typography::register(&mut config.encre_config);
    encre_css_icons::register(&mut config.encre_config);

    let mut buffer = String::new();

    if let Some(glob_path) = extra_input {
        scan_path(glob_path, &mut buffer);
    }

    config.input.iter().for_each(|glob_path| {
        scan_path(glob_path, &mut buffer);
    });

    gen_css([buffer.as_str()], &config.encre_config, output);
}

fn main() {
    if let Ok(_) = std::env::var("NIX_ENFORCE_PURITY") {
        exit(0)
    }
    println!("cargo:rerun-if-changed=assets/views");
    build_single(
        "encre.toml",
        Some("assets/views/**/*.html"),
        Some("assets/static/css/bundle.css".to_string()),
    );
}
