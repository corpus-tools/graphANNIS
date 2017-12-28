use stringstorage::StringStorage;
use annostorage::AnnoStorage;
use graphstorage::{WriteableGraphStorage, GraphStorage};
use {Component, NodeID, StringID, Edge, ComponentType};
use AnnoKey;
use graphstorage::registry;
use std::collections::{BTreeMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::io::prelude::*;
use std;
use strum::IntoEnumIterator;
use std::string::ToString;
use bincode;
use serde;



pub const ANNIS_NS: &str = "annis";
pub const NODE_NAME: &str = "node_name";
pub const TOK: &str = "tok";
pub const NODE_TYPE: &str = "node_type";


#[derive(Debug)]
pub enum Error {
    IOerror(std::io::Error),
    StringError(std::ffi::OsString),
    RegistryError(registry::RegistryError),
    SerializationError(bincode::Error),
    LocationEmpty,
    InvalidType,
    Other,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IOerror(e)
    }
}

impl From<registry::RegistryError> for Error {
    fn from(e: registry::RegistryError) -> Error {
        Error::RegistryError(e)
    }
}

impl From<std::ffi::OsString> for Error {
    fn from(e: std::ffi::OsString) -> Error {
        Error::StringError(e)
    }
}

impl From<bincode::Error> for Error {
    fn from(e: bincode::Error) -> Error {
        Error::SerializationError(e)
    }
}


pub struct GraphDB {
    pub strings: StringStorage,
    pub node_annos: AnnoStorage<NodeID>,

    location: Option<PathBuf>,

    components: BTreeMap<Component, Option<Rc<GraphStorage>>>,
    id_annis_ns: StringID,
    id_node_name: StringID,
    id_tok: StringID,
    id_node_type: StringID,
}

fn load_component_from_disk(component_path: Option<PathBuf> ) -> Result<Rc<GraphStorage>, Error> {
    let cpath = try!(component_path.ok_or(Error::LocationEmpty));
    
    // load component into memory
    let mut impl_path = PathBuf::from(&cpath);
    impl_path.push("impl.cfg");
    let mut f_impl = std::fs::File::open(impl_path)?;
    let mut impl_name = String::new();
    f_impl.read_to_string(&mut impl_name)?;

    let mut data_path = PathBuf::from(&cpath);
    data_path.push("data");
    let f_data = std::fs::File::open(data_path)?;
    let mut buf_reader = std::io::BufReader::new(f_data);
    let gs = registry::load_by_name(&impl_name, &mut buf_reader)?;

    return Ok(gs);
}


impl GraphDB {
    /// Create a new and empty instance without any location on the disk
    pub fn new() -> GraphDB {
        let mut strings = StringStorage::new();

        GraphDB {
            id_annis_ns: strings.add(ANNIS_NS),
            id_node_name: strings.add(NODE_NAME),
            id_tok: strings.add(TOK),
            id_node_type: strings.add(NODE_TYPE),

            strings,
            node_annos: AnnoStorage::<NodeID>::new(),
            components: BTreeMap::new(),

            location: None,
        }
    }

    pub fn clear(&mut self) {
        self.strings.clear();
        self.node_annos.clear();
        self.components.clear();
    }

    fn load_bincode<T>(&self, path : &str) -> Result<T, Error> 
        where for<'de> T: serde::Deserialize<'de> {
        if let Some(ref loc) = self.location {
            let mut full_path = PathBuf::from(loc);
            full_path.push(path);

            let f = std::fs::File::open(full_path)?;
            let mut reader = std::io::BufReader::new(f);
            let result : T  = bincode::deserialize_from(&mut reader, bincode::Infinite)?;
            return Ok(result);
        } else {
            return Err(Error::LocationEmpty);
        }
    }

    fn save_bincode<T>(&self, path : &str, object : &T) -> Result<(), Error> 
        where T: serde::Serialize {
        if let Some(ref loc) = self.location {
            let mut full_path = PathBuf::from(loc);
            full_path.push(path);

            let f = std::fs::File::open(full_path)?;
            let mut writer = std::io::BufWriter::new(f);
            bincode::serialize_into(&mut writer, object, bincode::Infinite)?;
            return Ok(());
        } else {
            return Err(Error::LocationEmpty);
        }
    }

    pub fn load(&mut self, location : &Path, preload: bool) -> Result<(), Error> {

        self.clear();

        // TODO: implement WAL support
        self.strings = self.load_bincode("current/strings.bin")?;
        self.node_annos = self.load_bincode("current/strings/nodes.bin")?;
        
        self.load_graph_storages(location, preload)?;

        Ok(())
    }

    fn load_graph_storages(&mut self, location : &Path, preload : bool) -> Result<(), Error> {
        self.components.clear();

        // for all component types
        for c in ComponentType::iter() {
            let mut component_path = PathBuf::new();
            component_path.push(location);
            component_path.push("gs");
            component_path.push(c.to_string());
            if component_path.is_dir() {
                // get all the namespaces/layers
                for layer in component_path.read_dir()? {
                    let layer = layer?;
                    // try to load the component with the empty name
                    let empty_name_component = Component {
                        ctype: c.clone(),
                        layer: layer.file_name().into_string()?,
                        name: String::from(""),
                    };
                }
            }
            // TODO: load components
        }

        Ok(())
    }

    pub fn save(&self) -> Result<(),Error> {
        self.save_bincode("current/strings.bin", &self.strings)?;
        self.save_bincode("current/nodes.bin", &self.node_annos)?;
        // TODO: save all loaded graph storages
        unimplemented!()
    }

    fn component_path(&self, c: &Component) -> Option<PathBuf> {
        match self.location {
            Some(ref loc) => {
                let mut p = PathBuf::from(loc);
                p.push("gs");
                p.push(c.ctype.to_string());
                p.push(&c.layer);
                p.push(&c.name);
                Some(p)
            }
            None => None,
        }
    }

    fn insert_or_copy_writeable(&mut self, c : &Component) ->Result<(), Error> {
        // move the old entry into the ownership of this function
        let entry = self.components.remove(c);
        // component exists?
        if entry.is_some() {
            let gs_opt = entry.unwrap();

            let mut loaded_comp : Rc<GraphStorage> = if gs_opt.is_none() {
                load_component_from_disk(self.component_path(c))?
            } else {
                gs_opt.unwrap()
            };

            // copy to writable implementation if needed
            let is_writable = {Rc::get_mut(&mut loaded_comp).ok_or(Error::Other)?.as_writeable().is_some()};

            let loaded_comp = if is_writable {
                loaded_comp
            } else {
                let mut gs_copy = registry::create_writeable();
                gs_copy.copy(loaded_comp.as_ref());
                Rc::from(gs_copy)
   
            };

            // (re-)insert the component into map again
            self.components.insert(c.clone(), Some(loaded_comp));
        }
        return Ok(());
    }

    pub fn get_or_create_writable(&mut self, c : Component) -> Result<&mut WriteableGraphStorage, Error> {
        

        if self.components.contains_key(&c) {
            // make sure the component is actually writable and loaded
            self.insert_or_copy_writeable(&c)?;
        } else {
            let w = registry::create_writeable();

            self.components.insert(c.clone(), Some(Rc::from(w)));
        }
        
        // get and return the reference to the entry
        let entry : &mut Rc<GraphStorage> = self.components.get_mut(&c).ok_or(Error::Other)?.as_mut() .ok_or(Error::Other)?;
        let gs_mut_ref : &mut GraphStorage = Rc::get_mut(entry).ok_or(Error::Other)?;
        return Ok(gs_mut_ref.as_writeable().ok_or(Error::InvalidType)?);

    }

    pub fn ensure_loaded(&mut self, c : &Component) -> Result<(), Error> {
        
        // get and return the reference to the entry if loaded
        let entry : Option<Option<Rc<GraphStorage>>> = self.components.remove(c);
        if let Some(gs_opt) = entry {
            
            let loaded : Rc<GraphStorage> = if gs_opt.is_none() {
                load_component_from_disk(self.component_path(c))?
            } else {
                gs_opt.unwrap()
            };

            self.components.insert(c.clone(), Some(loaded));
        }
        return Ok(());
    }

    pub fn get_graphstorage(&self, c : &Component) -> Option<Rc<GraphStorage>> {
        
        // get and return the reference to the entry if loaded
        let entry : Option<& Option<Rc<GraphStorage>>> = self.components.get(c);
        if let Some(gs_opt) = entry {
            if let Some(ref impl_type) = *gs_opt {
                return Some(impl_type.clone());
            }
        }
        return None;
    }

    pub fn get_direct_connected(&mut self, edge : &Edge) -> Result<Vec<Component>,Error> {
        let mut result = Vec::new();

        let all_components : Vec<Component> = self.components.keys().map(|c| c.clone()).collect();

        for c in all_components {
            self.ensure_loaded(&c)?;
            if let Some(gs) = self.get_graphstorage(&c) {
                if gs.is_connected(&edge.source, &edge.target, 1, 1) {
                    result.push(c.clone());
                }
            }
        }
        return Ok(result);
    }

    pub fn get_token_key(&self) -> AnnoKey {
        AnnoKey {
            ns: self.id_annis_ns,
            name: self.id_tok,
        }
    }

    pub fn get_node_name_key(&self) -> AnnoKey {
        AnnoKey {
            ns: self.id_annis_ns,
            name: self.id_node_name,
        }
    }

    pub fn get_node_type_key(&self) -> AnnoKey {
        AnnoKey {
            ns: self.id_annis_ns,
            name: self.id_node_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use {ComponentType, Edge, Annotation, AnnoKey};

    #[test]
    fn create_writeable_gs() {
        let mut db = GraphDB::new();
        
        let anno_key = AnnoKey{ns: db.strings.add("test"), name: db.strings.add("edge_anno")};
        let anno_val = db.strings.add("testValue");
        
        let gs : &mut WriteableGraphStorage = db.get_or_create_writable(Component{ctype: ComponentType::Pointing, layer:String::from("test"), name: String::from("dep")}).unwrap();

        gs.add_edge(Edge{source: 0, target: 1});
        
        gs.add_edge_annotation(Edge{source: 0, target: 1},
            Annotation{key: anno_key, val: anno_val});
        
    }
}