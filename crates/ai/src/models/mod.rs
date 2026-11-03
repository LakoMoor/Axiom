//! The hub of local models: what to run on this computer (a recommended list that knows how much
//! memory it has, and a search of Hugging Face), downloads checked against the hub's own hashes, and
//! `llama-server` to run them. A running model is added as an AI like any other.

pub mod catalog;
pub mod hf;
pub mod server;
pub mod store;
pub mod tools;
