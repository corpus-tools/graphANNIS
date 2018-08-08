extern crate graphannis_malloc_size_of as malloc_size_of;
#[macro_use]
extern crate graphannis_malloc_size_of_derive as malloc_size_of_derive;
#[macro_use]
extern crate log;

extern crate regex;
extern crate regex_syntax;
extern crate rand;
extern crate multimap;
extern crate linked_hash_map;
extern crate fxhash;
extern crate tempdir;

extern crate serde;
extern crate bincode;

extern crate csv;

extern crate strum;
#[macro_use]
extern crate strum_macros;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate lazy_static;

extern crate serde_json;

extern crate num;
extern crate itertools;
extern crate rayon;
extern crate sys_info;
extern crate fs2;

#[macro_use]
pub mod util;

mod types;
pub use types::*;

mod dfs;
pub mod annostorage;
pub mod stringstorage;
pub mod graphstorage;
pub mod graphdb;
pub mod operator;
pub mod relannis;
mod plan;
pub mod exec;
mod query;
pub mod parser;

pub mod api;

// Make sure the allocator is always the one from the system, otherwise we can't make sure our memory estimations work
use std::alloc::System;
#[global_allocator]
static GLOBAL: System = System;