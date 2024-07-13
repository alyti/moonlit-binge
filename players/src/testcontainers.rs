use std::{borrow::Cow, collections::HashMap, path::Path};

use testcontainers::{
    core::{ContainerPort, Mount, WaitFor},
    Image,
};

const JELLYFIN_NAME: &str = "ghcr.io/linuxserver/jellyfin";
const JELLYFIN_TAG: &str = "amd64-10.9.7ubu2204-ls20";
pub const JELLYFIN_HTTP_PORT: ContainerPort = ContainerPort::Tcp(8096);

#[derive(Debug, Clone)]
pub struct Jellyfin {
    env_vars: HashMap<String, String>,
    media_mount: Option<Mount>,
}

impl Default for Jellyfin {
    fn default() -> Self {
        let env_vars = [("PUID", "1000"), ("PGID", "1000"), ("TZ", "Etc/UTC")]
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();

        Self {
            env_vars,
            media_mount: None,
        }
    }
}

impl Image for Jellyfin {
    fn name(&self) -> &str {
        JELLYFIN_NAME
    }

    fn tag(&self) -> &str {
        JELLYFIN_TAG
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stdout("Startup complete")]
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<Item = (impl Into<Cow<'_, str>>, impl Into<Cow<'_, str>>)> {
        &self.env_vars
    }

    fn mounts(&self) -> impl IntoIterator<Item = &Mount> {
        let mut mounts = Vec::new();
        if let Some(conf_mount) = &self.media_mount {
            mounts.push(conf_mount);
        }
        mounts
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[JELLYFIN_HTTP_PORT]
    }
}

impl Jellyfin {
    #[must_use]
    pub fn with_media_mount(self, media_mount_path: impl AsRef<Path>) -> Self {
        Self {
            media_mount: Some(Mount::bind_mount(
                media_mount_path.as_ref().to_str().unwrap_or_default(),
                "/media",
            )),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use testcontainers::runners::AsyncRunner;

    use crate::testcontainers::{Jellyfin as JellyfinContainer, JELLYFIN_HTTP_PORT};

    #[tokio::test]
    async fn it_works() -> Result<(), Box<dyn std::error::Error + 'static>> {
        let container = JellyfinContainer::default().start().await?;
        let host = container.get_host().await?;
        let host_port = container.get_host_port_ipv4(JELLYFIN_HTTP_PORT).await?;
        let url = format!("http://{host}:{host_port}");

        let client = crate::jellyfin::Jellyfin::new(&url, &None);
        client.ping().await?;
        match client.complete_startup("root", None).await {
            Ok(_) => {}
            Err(e) => {
                println!("Jellyfin is not ready: {}", e);
                let stdout = container.stdout_to_vec().await?;
                println!(
                    "idk just check the container logs for now: {}",
                    std::str::from_utf8(&stdout).unwrap()
                );
                assert_eq!("1", "");
            }
        }

        // linuxserver defaults to abc user
        let user = client.authenticate("abc", "root").await?;
        // make sure qc works
        let qc = client.new_quick_connect().await?;
        user.authorize(&qc.code).await?;
        let user2 = qc.auth().await?;
        assert_eq!(user.id, user2.id);

        Ok(())
    }
}
