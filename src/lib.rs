pub mod error;
mod http_client;

use http::StatusCode;
use url;
use mockall_double::double;
use reqwest::blocking::Response;
use serde_json::{json, Value};

#[double]
use http_client::HttpClient;
use crate::error::RSCError;

#[derive(Clone)]
pub enum Payload {
    Body(Value),
    Empty,
    None
}

#[non_exhaustive]
pub struct RequestHandlers;

impl RequestHandlers {
    pub const QUERY: &'static str = "select";
    pub const CREATE: &'static str = "update/json/docs";
    pub const DELETE: &'static str = "update";
}

pub struct Client<'a> {
    request_handler: &'a str,
    url: url::Url,
    payload: Payload,
    collection: &'a str
}

impl<'a> Client<'a> {

    pub fn new(base_url: &'a str, collection: &'a str) -> Self {
        let url = url::Url::parse(base_url).unwrap();
        Client { request_handler: "", url, payload: Payload::None, collection }
    }

    pub fn add_query_param(&mut self, key: &str, value: &str) -> &mut Self {
        self.url.query_pairs_mut().append_pair(key, value);
        self
    }

    pub fn request_handler(&mut self, handler: &'a str) -> &mut Self {
        self.request_handler = handler;
        self.payload(Payload::None);
        self.url.path_segments_mut().unwrap()
            .clear()
            .push("solr")
            .push(self.collection)
            .push(self.request_handler);
        self
    }

    pub fn auto_commit(&mut self) -> &mut Self {
        self.add_query_param("commit", "true");
        self
    }

    pub fn query(&mut self, query: &str) -> &mut Self {
        self.add_query_param("q", query);
        self
    }

    pub fn generate_url_str(&self) -> &str {
        self.url.as_str()
    }

    pub fn payload(&mut self, payload: Payload) -> &mut Self {
        self.payload = payload;
        self
    }

    pub fn run(&mut self) -> Result<Value, RSCError> {
        let solr_result = match self.payload.clone() {
            Payload::Body(body) => HttpClient::new().post(self.generate_url_str(), Some(body)),
            Payload::Empty => HttpClient::new().post(self.generate_url_str(), None),
            _ => HttpClient::new().get(self.generate_url_str())
        };

        let response = match solr_result {
            Ok(response) => response,
            Err(e) => return Err(RSCError { source: Some(Box::new(e)), status: None, message: None }),
        };

        self.url.query_pairs_mut().clear();

        self.handle_response(response)
    }

    pub fn select(&mut self, query: &str) -> &mut Self {
        self
            .request_handler(RequestHandlers::QUERY)
            .query(query)
    }

    pub fn create(&mut self, document: Value) -> &mut Self {
        self
            .request_handler(RequestHandlers::CREATE)
            .payload(Payload::Body(document))
    }

    pub fn delete(&mut self, query: &str) -> &mut Self {
        let delete_payload = json!({
            "delete": { "query": query }
        });

        self
            .request_handler(RequestHandlers::DELETE)
            .payload(Payload::Body(delete_payload))
    }

    pub fn commit(&mut self) -> &mut Self {
        self
            .request_handler("update")
            .auto_commit()
            .payload(Payload::Empty)
    }

    fn handle_response(&self, response: Response) -> Result<Value, RSCError> {
        match response.status() {
            StatusCode::OK => Ok(response.json::<Value>().unwrap()["response"]["docs"].clone()),
            StatusCode::NOT_FOUND => return Err(RSCError { source: None, status: Some(StatusCode::NOT_FOUND), message: None }),
            other_status => {
                let body_text = response.text().unwrap();
                let message_string = match serde_json::from_str::<Value>(&body_text) {
                    Ok(r) => r["error"]["msg"].to_string(),
                    Err(e) => {
                        return Err(
                            RSCError {
                                source: Some(Box::new(e)),
                                status: Some(other_status),
                                message: Some(body_text)
                            })
                    }
                };
                return Err(RSCError { source: None, status: Some(other_status), message: Some(message_string.replace("\"", "")) })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Mutex, MutexGuard};
    use mockall::lazy_static;
    use mockall::predicate::eq;
    use serde_json::json;
    use crate::error;

    lazy_static! {
        static ref MTX: Mutex<()> = Mutex::new(());
    }

    fn get_lock(m: &'static Mutex<()>) -> MutexGuard<'static, ()> {
        match m.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[test]
    fn test_build_a_url_from_parameters() {
        let mut params = Client::new("http://host:8983", "collection");
        params
            .request_handler("request_handler")
            .query("*:*");

        let url_string = params.generate_url_str();
        assert_eq!(url_string, "http://host:8983/solr/collection/request_handler?q=*%3A*");
    }

    #[test]
    fn test_build_a_url_from_parameters_set_autocommit() {
        let mut params = Client::new("http://host:8983", "collection");
        params
            .request_handler("request_handler")
            .auto_commit();

        let url_string = params.generate_url_str();
        assert_eq!(url_string, "http://host:8983/solr/collection/request_handler?commit=true");
    }

    #[test]
    fn test_run_calls_get_with_url() {
        let _m = get_lock(&MTX);

        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_get()
                .with(eq("http://localhost:8983/solr/default/select?q=*%3A*"))
                .returning(|_| Ok(reqwest::blocking::Response::from(http::response::Builder::new()
                    .status(200)
                    .body(r#"{"response": {"docs": [{"success": true}]}}"#)
                    .unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let mut command = Client::new(host, collection);
        let result = command
            .request_handler("select")
            .query("*:*")
            .run();
        assert!(result.is_ok());
        assert_eq!(result.unwrap()[0]["success"], true);
    }

    #[test]
    fn test_run_calls_post_with_url_and_body() {
        let _m = get_lock(&MTX);

        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_post()
                .withf(| url, body | url == "http://localhost:8983/solr/default/update%2Fjson%2Fdocs?commit=true" && *body == Some(json!({ "this is": "a document"})) )
                .returning(|_, _| Ok(reqwest::blocking::Response::from(http::response::Builder::new()
                    .status(200)
                    .body(r#"{"response": {"docs": [{"success": true}]}}"#)
                    .unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let mut command = Client::new(host, collection);
        let result = command
            .request_handler("update/json/docs")
            .auto_commit()
            .payload(Payload::Body(json!({ "this is": "a document"})))
            .run();
        assert!(result.is_ok());
        assert_eq!(result.unwrap()[0]["success"], true);
    }

    #[test]
    fn test_select_responds_rsc_error_with_other_problem_if_dunno() {
        let _m = get_lock(&MTX);
        let ctx = HttpClient::new_context();

        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_get().returning(|_| Ok(reqwest::blocking::Response::from(http::response::Builder::new().status(500).body(r#"{"error": {"code": 500, "msg": "okapi"}}"#).unwrap())));
            mock
        });

        let collection = "default";
        let base_url = "http://localhost:8983";
        let result = Client::new(base_url, collection)
            .select("bad: query")
            .run();
        assert!(result.is_err());
        let error = result.err().expect("No Error");
        assert_eq!(error.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message().unwrap(), "okapi");
        assert!(matches!(error.kind(), error::ErrorKind::Other));
    }

    #[test]
    fn test_select_responds_rsc_error_with_raw_text_body_and_status_code_if_no_standard_message() {
        let _m = get_lock(&MTX);
        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_get().returning(|_| Ok(reqwest::blocking::Response::from(http::response::Builder::new().status(500).body(r#"some unparseable thing"#).unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let result = Client::new(host, collection)
            .select("bad: query")
            .run();
        assert!(result.is_err());
        let error = result.err().expect("No Error");
        assert_eq!(error.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message().unwrap(), "some unparseable thing");
        assert!(matches!(error.kind(), error::ErrorKind::Other));
    }

    #[test]
    fn test_create_responds_rsc_error_with_other_problem_if_dunno() {
        let _m = get_lock(&MTX);
        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_post().returning(|_, _| Ok(reqwest::blocking::Response::from(http::response::Builder::new().status(500).body(r#"{"error": {"code": 500, "msg": "okapi"}}"#).unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let result = Client::new(host, collection)
            .auto_commit()
            .create(json!({"anything": "anything"}))
            .run();
        assert!(result.is_err());
        let error = result.err().expect("No Error");
        assert_eq!(error.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message().unwrap(), "okapi");
        assert!(matches!(error.kind(), error::ErrorKind::Other));
    }

    #[test]
    fn test_create_responds_rsc_error_with_raw_text_body_and_status_code_if_no_standard_message() {
        let _m = get_lock(&MTX);
        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_post().returning(|_, _| Ok(reqwest::blocking::Response::from(http::response::Builder::new().status(500).body(r#"some unparseable thing"#).unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let result = Client::new(host, collection)
            .auto_commit()
            .create(json!({"anything": "anything"}))
            .run();
        assert!(result.is_err());
        let error = result.err().expect("No Error");
        assert_eq!(error.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message().unwrap(), "some unparseable thing");
        assert!(matches!(error.kind(), error::ErrorKind::Other));
    }

    #[test]
    fn test_delete_responds_rsc_error_with_other_problem_if_dunno() {
        let _m = get_lock(&MTX);
        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_post().returning(|_, _| Ok(reqwest::blocking::Response::from(http::response::Builder::new().status(500).body(r#"{"error": {"code": 500, "msg": "okapi"}}"#).unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let result = Client::new(host, collection)
            .auto_commit()
            .delete("*:*")
            .run();
        assert!(result.is_err());
        let error = result.err().expect("No Error");
        assert_eq!(error.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message().unwrap(), "okapi");
        assert!(matches!(error.kind(), error::ErrorKind::Other));
    }

    #[test]
    fn test_delete_responds_rsc_error_with_raw_text_body_and_status_code_if_no_standard_message() {
        let _m = get_lock(&MTX);
        let ctx = HttpClient::new_context();
        ctx.expect().returning(|| {
            let mut mock = HttpClient::default();
            mock.expect_post().returning(|_, _| Ok(reqwest::blocking::Response::from(http::response::Builder::new().status(500).body(r#"some unparseable thing"#).unwrap())));
            mock
        });

        let collection = "default";
        let host = "http://localhost:8983";
        let result = Client::new(host, collection)
            .delete("*:*")
            .run();
        assert!(result.is_err());
        let error = result.err().expect("No Error");
        assert_eq!(error.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message().unwrap(), "some unparseable thing");
        assert!(matches!(error.kind(), error::ErrorKind::Other));
    }
}