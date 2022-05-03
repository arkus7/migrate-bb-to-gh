use async_trait::async_trait;
use reqwest::header::HeaderMap;
use reqwest::{IntoUrl, Method, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub(crate) struct BasicAuth<'a>(&'a str, &'a str);
impl<'a> BasicAuth<'a> {
    pub fn new(username: &'a str, password: &'a str) -> Self {
        Self(username, password)
    }
}

#[async_trait]
pub(crate) trait ApiClient {
    fn basic_auth(&self) -> Option<BasicAuth>;
    fn headers(&self) -> Option<HeaderMap>;

    async fn get<T, U>(&self, url: U) -> reqwest::Result<T>
    where
        T: DeserializeOwned,
        U: IntoUrl + Send,
    {
        self.request(Method::GET, url, Option::<serde_json::Value>::None)
            .await
    }

    async fn post<T, U, B>(&self, url: U, body: Option<B>) -> reqwest::Result<T>
    where
        T: DeserializeOwned,
        U: IntoUrl + Send,
        B: Serialize + Send,
    {
        self.request(Method::POST, url, body).await
    }

    async fn put<T, U, B>(&self, url: U, body: Option<B>) -> reqwest::Result<T>
    where
        T: DeserializeOwned,
        U: IntoUrl + Send,
        B: Serialize + Send,
    {
        self.request(Method::PUT, url, body).await
    }

    async fn patch<T, U, B>(&self, url: U, body: Option<B>) -> reqwest::Result<T>
    where
        T: DeserializeOwned,
        U: IntoUrl + Send,
        B: Serialize + Send,
    {
        self.request(Method::PATCH, url, body).await
    }

    async fn request<T, U, B>(&self, method: Method, url: U, body: Option<B>) -> reqwest::Result<T>
    where
        T: DeserializeOwned,
        U: IntoUrl + Send,
        B: Serialize + Send,
    {
        let client = reqwest::Client::new().request(method, url);
        let mut builder = self.build_common_parts(client);
        if let Some(body) = body {
            builder = builder.json(&body);
        }

        let response = builder.send().await?.error_for_status()?;

        let mut body = response.text().await?;
        if body.is_empty() {
            body = "{}".to_string();
        }

        let response = serde_json::from_str(&body).unwrap();

        Ok(response)
    }

    #[inline]
    fn build_common_parts(&self, builder: RequestBuilder) -> RequestBuilder {
        let mut builder = builder;
        if let Some(headers) = self.headers() {
            builder = builder.headers(headers);
        }
        if let Some(basic) = self.basic_auth() {
            builder = builder.basic_auth(basic.0, Some(basic.1));
        }

        builder
    }
}
