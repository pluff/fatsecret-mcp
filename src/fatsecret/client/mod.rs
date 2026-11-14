pub mod base;
pub mod diary;
pub mod error;
pub mod exercises;
pub mod favorites;
pub mod foods;
pub mod profile;
pub mod recipes;
pub mod reference;
pub mod saved_meals;
pub mod weight;

pub use base::FatSecretClient;
pub use error::{FatSecretError, Result};
