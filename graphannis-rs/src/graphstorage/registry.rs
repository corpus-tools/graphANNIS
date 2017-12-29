use graphstorage::{GraphStorage};
use super::adjacencylist::AdjacencyListStorage;
use std;
use std::rc::Rc;
use bincode;
use std::any::Any;
use std::str::FromStr;
use strum;

#[derive(Debug)]
pub enum RegistryError {
    Empty,
    ImplementationNameNotFound,
    TypeNotFound,
    Serialization(Box<bincode::ErrorKind>),
    Other,
}

#[derive(ToString, EnumString)]
enum ImplTypes {
    AdjacencyV1,
}

impl From<Box<bincode::ErrorKind>> for RegistryError {
    fn from(e: Box<bincode::ErrorKind>) -> RegistryError {
        RegistryError::Serialization(e)
    }
}

impl From<strum::ParseError> for RegistryError {
    fn from(e: strum::ParseError) -> RegistryError {
        RegistryError::ImplementationNameNotFound
    }
}

type Result<T> = std::result::Result<T, RegistryError>;

pub fn create_writeable() -> AdjacencyListStorage {
    // TODO: make this configurable when there are more writeable graph storage implementations
    AdjacencyListStorage::new()
}

pub fn load_by_name(impl_name : &str, input : &mut std::io::Read) -> Result<Rc<GraphStorage>> {

    let impl_type = ImplTypes::from_str(impl_name)?;

    match impl_type {
        ImplTypes::AdjacencyV1 => {
            let gs : AdjacencyListStorage =  bincode::deserialize_from(input, bincode::Infinite)?;
            Ok(Rc::new(gs))
        }
    }
}

pub fn serialize(data : &Any, writer : &mut std::io::Write) -> Result<String> {
    if let Some(adja) = data.downcast_ref::<Rc<AdjacencyListStorage>>() {
        bincode::serialize_into(writer, adja.as_ref(), bincode::Infinite)?;
        return Ok(ImplTypes::AdjacencyV1.to_string());
    }
    return Err(RegistryError::TypeNotFound);
}


