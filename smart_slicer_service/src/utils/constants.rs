// Constants used across the project

use dotenv::dotenv;
use lazy_static::lazy_static;
use std::env;

lazy_static! {
    /// Directory where log files will be written
    pub static ref LOG_DIR: String = {
        dotenv().ok();
        env::var("LOG_DIR").unwrap_or_else(|_| "logs".into())
    };
}
