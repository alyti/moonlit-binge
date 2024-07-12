use axum::{async_trait, Extension, Router as AxumRouter};
use fluent_templates::{ArcLoader, FluentLoader};
use loco_rs::{
    app::{AppContext, Initializer},
    controller::views::ViewEngine,
    prelude::ViewRenderer,
    Error, Result,
};
use tracing::info;

const I18N_DIR: &str = "assets/i18n";
const I18N_SHARED: &str = "assets/i18n/shared.ftl";

use std::path::Path;

use serde::Serialize;

const VIEWS_DIR: &str = "assets/views/";
const VIEWS_GLOB: &str = "assets/views/**/*.html";

#[derive(Clone, Debug)]
pub struct BetterTeraView {
    pub tera: tera::Tera,
    pub default_context: tera::Context,
}

impl BetterTeraView {
    /// Create a Tera view engine
    ///
    /// # Errors
    ///
    /// This function will return an error if building fails
    pub fn build() -> Result<Self> {
        if !Path::new(VIEWS_DIR).exists() {
            return Err(Error::string(&format!(
                "missing views directory: `{VIEWS_DIR}`"
            )));
        }

        let tera = tera::Tera::new(VIEWS_GLOB)?;
        let ctx = tera::Context::default();
        Ok(Self {
            tera,
            default_context: ctx,
        })
    }
}

impl ViewRenderer for BetterTeraView {
    fn render<S: Serialize>(&self, key: &str, data: S) -> Result<String> {
        let mut context = tera::Context::from_serialize(data)?;
        context.extend(self.default_context.clone());

        Ok(self.tera.render(key, &context)?)
    }
}

pub struct ViewEngineInitializer;
#[async_trait]
impl Initializer for ViewEngineInitializer {
    fn name(&self) -> String {
        "view-engine".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        let mut tera_engine = BetterTeraView::build()?;
        if std::path::Path::new(I18N_DIR).exists() {
            let arc = ArcLoader::builder(&I18N_DIR, unic_langid::langid!("en-US"))
                .shared_resources(Some(&[I18N_SHARED.into()]))
                .customize(|bundle| bundle.set_use_isolating(false))
                .build()
                .map_err(|e| Error::string(&e.to_string()))?;
            tera_engine
                .tera
                .register_function("t", FluentLoader::new(arc));
            info!("locales loaded");
        }

        Ok(router.layer(Extension(ViewEngine::from(tera_engine))))
    }
}
