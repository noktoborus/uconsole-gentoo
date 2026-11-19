//! drowse: the code of the service `drowsed` and of the tools of the user,
//! `drowse-inhibit` and `drowse-status`. A program only reads its
//! arguments and runs this library.

pub mod bus;
pub mod changes;
pub mod engine;
pub mod error;
pub mod inhibit;
pub mod journal;
pub mod language;
pub mod login;
pub mod service;
pub mod settings;
pub mod stage;
pub mod status;
pub mod world;

pub use error::{Error, Result};

rust_i18n::i18n!("locales", fallback = "en");
