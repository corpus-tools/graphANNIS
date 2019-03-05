#![allow(deprecated)]

use crate::annis::types::Component;
use crate::annis::types::LineColumnRange;

error_chain! {

    foreign_links {
        CSV(::csv::Error);
        IO(::std::io::Error);
        ParseIntError(::std::num::ParseIntError);
        Bincode(::bincode::Error);
        Fmt(::std::fmt::Error);
        Strum(::strum::ParseError);
        Regex(::regex::Error);
    }

    errors {
        LoadingComponentFailed(c: Component) {
            description("Could not load component from disk"),
            display("Could not load component {} from disk", c),
        }

        LoadingDBFailed(db : String) {
            description("Could not load graph from disk"),
            display("Could not load graph {} from disk", &db),
        }

        ImpossibleSearch(reason : String) {
            description("Impossible search expression detected"),
            display("Impossible search expression detected: {}", reason),
        }

        NoSuchString(val : String) {
            description("String does not exist"),
            display("String '{}' does not exist", &val),
        }

        NoSuchCorpus(name : String) {
            description("NoSuchCorpus"),
            display("Corpus {} not found", &name)
        }

        AQLSyntaxError(msg : String, location : Option<LineColumnRange>) {
            description("AQLSyntaxError"),
            display("{}", {
                if let Some(location) = location {
                    format!("[{}] {}", &location, msg)
                } else {
                    msg.to_string()
                }

            }),
        }

        AQLSemanticError(msg : String, location : Option<LineColumnRange>) {
            description("AQLSemanticError"),
            display("{}", {
                if let Some(location) = location {
                    format!("[{}] {}", &location, msg)
                } else {
                    msg.to_string()
                }

            }),
        }
    }
}
