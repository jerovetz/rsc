### RSC

A Solr client for Rust.

`Rsc` provides capabilities to manipulate and form
requests to the Solr server, and contains some shorthands
for them. It uses the blocking version of the reqwest http client.

 ## Query

 You can retrieve documents as types with implemented `Clone` and `Deserialize`.

 ```rust
 use serde_json::Value;
 use rsc::Client;
 use rsc::error::RSCError;
 use rsc::solr_result::SolrResult;

 fn query_all() -> Result<SolrResult<Value>, RSCError> {
     let result = Client::new("http://solr:8983", "collection")
         .select("*:*")
         .run::<Value>();
     match result {
         Ok(solr_result) => Ok(solr_result.expect("Request is OK, but no response; in select it's a failure on Solr side.")),
         Err(e) => Err(e)
     }
 }
 ```

 ## Create

 You can use types with implemented `Clone` and `Serialize`.

 ```rust

 use serde::Serialize;
 use serde_json::Value;
 use rsc::Client;

 #[derive(Serialize, Clone)]
 struct SimpleDocument {
     field: Vec<String>
 }

 fn create() {
     let document = SimpleDocument { field: vec!("nice".to_string(), "document".to_string()) };
     Client::new("http://solr:8983", "collection")
         .create(document)
         .run::<Value>().expect("panic, request failed.");
 }
 ```
 ## Delete

 ```rust
 use serde_json::Value;
 use rsc::Client;
 fn delete() {
     Client::new("http::/solr:8983", "collection")
         .delete("delete:query")
         .run::<Value>().expect("panic, request failed.");
 }
 ```

 ## Custom handler with params

 You can define any handlers as well.

 ```rust

 use serde_json::Value;
 use rsc::Client;
 use rsc::error::RSCError;
 use rsc::solr_result::SolrResult;
 fn more_like_this()  -> Result<SolrResult<Value>, RSCError> {
     let result = Client::new("http://solr:8983", "collection")
         .request_handler("mlt")
         .add_query_param("mlt.fl", "similarity_field")
         .add_query_param("mlt.mintf", "4")
         .add_query_param("mlt.minwl", "3")
         .run::<Value>();
     match result {
         Ok(solr_result) => Ok(solr_result.expect("Request is OK, but no response; in select it's a failure on Solr side.")),
         Err(e) => Err(e)
     }
 }
 ```