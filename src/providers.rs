use std::{future::Future, pin::Pin};

pub type ProviderResult = Result<String, String>;
pub type ProviderFuture<'a> = Pin<Box<dyn Future<Output=ProviderResult> + Send + 'a>>;

pub trait AiProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn default_model(&self) -> &'static str;
    fn chat<'a>(&'a self, client: &'a reqwest::Client, model: &'a str, messages: &'a [crate::Message]) -> ProviderFuture<'a>;
}
