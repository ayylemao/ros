use core::fmt::Debug;

use alloc::{
    borrow::ToOwned,
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::Once;
use sys::syscall::{errors::FsError, FileKindTag};
use tar_no_std::TarArchiveRef;

use crate::{
    fs::{DirEntry, FsBackend},
    utils::irq_lock::IrqMutex,
};

#[derive(Debug, Clone)]
pub enum Node {
    File {
        name: String,
        data: Vec<u8>,
        parent: Option<usize>,
    },
    Dir {
        name: String,
        children: BTreeMap<String, usize>,
        parent: Option<usize>,
    },
}

static RAMFS: Once<IrqMutex<Ramfs>> = Once::new();
pub static ROOT_NODE_ID: usize = 0;

#[derive(Debug, Clone)]
pub struct Ramfs {
    pub nodes: Vec<Node>,
}

static ARCHIVE: &[u8] = include_bytes!("../../../initrfs.tar");

impl Ramfs {
    fn new() -> Self {
        let mut rfs = Self { nodes: Vec::new() };
        rfs.nodes.push(Node::Dir {
            name: "/".to_string(),
            children: BTreeMap::new(),
            parent: None,
        });
        rfs
    }

    pub fn ramfs() -> &'static IrqMutex<Ramfs> {
        RAMFS.call_once(|| IrqMutex::new(Ramfs::new()))
    }

    fn split_parent_base(path: &str) -> Result<(&str, &str), FsError> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidPath);
        }
        if path == "/" {
            return Err(FsError::InvalidPath);
        }

        let path = path.trim_end_matches('/');
        let (parent, base) = path.rsplit_once('/').unwrap();
        let parent = if parent.is_empty() { "/" } else { parent };
        if base.is_empty() {
            return Err(FsError::InvalidPath);
        }
        Ok((parent, base))
    }

    pub fn create_file(&mut self, path: &str) -> Result<(), FsError> {
        self.ensure_abs(path)?;
        let (parent_path, fname) = Self::split_parent_base(path)?;
        let parent_id = self.lookup_dir(parent_path)?;

        {
            let children = self.get_dir_children_mut(parent_id)?;
            if let Some(existing) = children.get(fname).copied() {
                return match &self.nodes[existing] {
                    Node::Dir { .. } => Err(FsError::AlreadyExistsDir),
                    Node::File { .. } => Err(FsError::AlreadyExistsFile),
                };
            }
        }

        let new_id = self.nodes.len();
        self.nodes.push(Node::File {
            name: fname.to_string(),
            data: Vec::new(),
            parent: Some(parent_id),
        });

        let children = self.get_dir_children_mut(parent_id)?;
        children.insert(fname.to_string(), new_id);

        Ok(())
    }

    pub fn write_file_from_path(&mut self, path: &str, bytes: &[u8]) -> Result<(), FsError> {
        let id = self.lookup_file(path)?;
        match &mut self.nodes[id] {
            Node::File { data, .. } => {
                data.clear();
                data.extend_from_slice(bytes);
                Ok(())
            }
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    pub fn lookup_file(&self, path: &str) -> Result<usize, FsError> {
        let id = self.lookup_abs(path)?;
        self.lookup_file_fd(id)
    }

    pub fn lookup_file_fd(&self, id: usize) -> Result<usize, FsError> {
        match &self.nodes[id] {
            Node::File { .. } => Ok(id),
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    #[inline]
    fn ensure_abs(&self, path: &str) -> Result<(), FsError> {
        if path.starts_with('/') {
            Ok(())
        } else {
            Err(FsError::InvalidPath)
        }
    }

    fn get_dir_children_mut(&mut self, id: usize) -> Result<&mut BTreeMap<String, usize>, FsError> {
        match &mut self.nodes[id] {
            Node::Dir { children, .. } => Ok(children),
            Node::File { .. } => Err(FsError::NotADirectory),
        }
    }

    fn get_parent(&self, id: usize) -> Result<Option<usize>, FsError> {
        match &self.nodes[id] {
            Node::Dir { parent, .. } => Ok(*parent),
            Node::File { .. } => Err(FsError::NotADirectory),
        }
    }

    fn get_or_create_dir_child(&mut self, parent: usize, name: &str) -> Result<usize, FsError> {
        // look up
        let existing = {
            let children = self.get_dir_children_mut(parent)?;
            children.get(name).copied()
        };

        if let Some(id) = existing {
            match &self.nodes[id] {
                Node::Dir { .. } => Ok(id),
                Node::File { .. } => Err(FsError::AlreadyExistsFile),
            }
        } else {
            // create
            let new_id = self.nodes.len();
            self.nodes.push(Node::Dir {
                name: name.to_string(),
                children: BTreeMap::new(),
                parent: Some(parent),
            });

            // link
            let children = self.get_dir_children_mut(parent)?;
            children.insert(name.to_string(), new_id);

            Ok(new_id)
        }
    }

    pub fn mkdir_p(&mut self, path: &str) -> Result<(), FsError> {
        self.ensure_abs(path)?;

        let mut curr: usize = 0;

        for c in path.split('/').filter(|c| !c.is_empty()) {
            if c == "." {
                continue;
            }
            if c == ".." {
                curr = self.get_parent(curr)?.unwrap_or(0);
                continue;
            }
            curr = self.get_or_create_dir_child(curr, c)?;
        }

        Ok(())
    }

    pub fn lookup_abs(&self, path: &str) -> Result<usize, FsError> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidPath);
        }
        self.lookup_from(0, path)
    }

    pub fn lookup_from(&self, start: usize, path: &str) -> Result<usize, FsError> {
        let mut curr = start;

        for c in path.split('/').filter(|c| !c.is_empty()) {
            match c {
                "." => continue,
                ".." => {
                    curr = match &self.nodes[curr] {
                        Node::Dir { parent, .. } => parent.unwrap_or(curr),
                        Node::File { .. } => return Err(FsError::NotADirectory),
                    };
                }
                name => {
                    curr = match &self.nodes[curr] {
                        Node::Dir { children, .. } => {
                            children.get(name).copied().ok_or(FsError::NotFound)?
                        }
                        Node::File { .. } => return Err(FsError::NotADirectory),
                    };
                }
            }
        }

        Ok(curr)
    }

    pub fn lookup_dir(&self, path: &str) -> Result<usize, FsError> {
        let id = self.lookup_abs(path)?;
        match &self.nodes[id] {
            Node::Dir { .. } => Ok(id),
            Node::File { .. } => Err(FsError::NotADirectory),
        }
    }

    pub fn load_initrd(&mut self) -> Result<(), FsError> {
        let tar_archive = TarArchiveRef::new(ARCHIVE).unwrap();
        let entries = tar_archive.entries().collect::<Vec<_>>();
        for entry in entries {
            let path = entry.filename();
            let path: &str = path.as_str_until_first_space().unwrap();
            let mut path = String::from(path.split_once("/").unwrap().1);
            path.insert_str(0, "/");

            let (parent, _fname) = Self::split_parent_base(&path)?;
            match self.mkdir_p(parent) {
                Ok(_v) => {}
                Err(e) => match e {
                    FsError::AlreadyExistsDir => {}
                    _ => {
                        return Err(e);
                    }
                },
            }

            match self.create_file(&path) {
                Ok(_v) => {
                    let fcontent = entry.data();
                    match self.write_file_from_path(&path, fcontent) {
                        Ok(_v) => {}
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn create_kernel_files(&mut self) -> Result<(), FsError> {
        self.mkdir_p("/var/log")?;
        self.create_file("/var/log/messages")?;
        Ok(())
    }

    pub fn path_of(&self, mut id: usize) -> String {
        if id == 0 {
            return "/".to_string();
        }

        let mut parts: Vec<&str> = Vec::new();
        while id != 0 {
            match &self.nodes[id] {
                Node::Dir { name, parent, .. } | Node::File { name, parent, .. } => {
                    parts.push(name.as_str());
                    id = parent.unwrap_or(0);
                }
            }
        }

        parts.reverse();
        let mut s = String::from("/");
        s.push_str(&parts.join("/"));
        s
    }
}

impl FsBackend for Ramfs {
    type NodeId = usize;

    fn root(&self) -> Self::NodeId {
        return ROOT_NODE_ID;
    }

    fn node_type(&self, id: Self::NodeId) -> Result<FileKindTag, FsError> {
        let node = self.nodes.get(id).ok_or(FsError::NotFound)?;
        match node {
            Node::Dir { .. } => Ok(FileKindTag::Dir),
            Node::File { .. } => Ok(FileKindTag::File),
        }
    }

    fn parent(&self, id: Self::NodeId) -> Result<Option<Self::NodeId>, FsError> {
        let node = self.nodes.get(id).ok_or(FsError::NotFound)?;
        match node {
            Node::Dir { parent, .. } => Ok(*parent),
            Node::File { parent, .. } => Ok(*parent),
        }
    }

    fn lookup(&self, dir: Self::NodeId, name: &str) -> Result<Self::NodeId, FsError> {
        let node = self.nodes.get(dir).ok_or(FsError::NotFound)?;
        match node {
            Node::Dir { children, .. } => children.get(name).copied().ok_or(FsError::NotFound),
            Node::File { .. } => Err(FsError::NotADirectory),
        }
    }

    fn name(&self, id: Self::NodeId) -> Result<String, FsError> {
        let node = self.nodes.get(id).ok_or(FsError::NotFound)?;
        match node {
            Node::Dir { name, .. } => Ok(name.to_owned()),
            Node::File { name, .. } => Ok(name.to_owned()),
        }
    }

    fn readdir(&self, dir: Self::NodeId) -> Result<Vec<DirEntry>, FsError> {
        let mut dir_entries: Vec<DirEntry> = Vec::new();
        let node = self.nodes.get(dir).ok_or(FsError::NotFound)?;
        match node {
            Node::Dir { children, .. } => {
                for (fname, node_id) in children.iter() {
                    let ftype = match &self.nodes[*node_id] {
                        Node::Dir { .. } => FileKindTag::Dir,
                        Node::File { .. } => FileKindTag::File,
                    };
                    dir_entries.push(DirEntry {
                        id: *node_id,
                        name: fname.clone(),
                        ftype: ftype,
                    });
                }
            }
            Node::File { .. } => return Err(FsError::NotADirectory),
        };
        Ok(dir_entries)
    }

    fn mkdir(&mut self, dir: Self::NodeId, name: &str) -> Result<Self::NodeId, FsError> {
        // look up
        let existing = {
            let children = self.get_dir_children_mut(dir)?;
            children.get(name).copied()
        };

        if let Some(id) = existing {
            match &self.nodes[id] {
                Node::Dir { .. } => Ok(id),
                Node::File { .. } => Err(FsError::AlreadyExistsFile),
            }
        } else {
            // create
            let new_id = self.nodes.len();
            self.nodes.push(Node::Dir {
                name: name.to_string(),
                children: BTreeMap::new(),
                parent: Some(dir),
            });

            // link
            let children = self.get_dir_children_mut(dir)?;
            children.insert(name.to_string(), new_id);

            Ok(new_id)
        }
    }

    fn create(&mut self, dir: Self::NodeId, name: &str) -> Result<Self::NodeId, FsError> {
        let existing = match self.nodes.get(dir).ok_or(FsError::NotFound)? {
            Node::Dir { children, .. } => children.get(name).copied(),
            Node::File { .. } => return Err(FsError::NotADirectory),
        };

        if let Some(id) = existing {
            return match self.nodes.get(id).unwrap() {
                Node::Dir { .. } => Err(FsError::AlreadyExistsDir),
                Node::File { .. } => Err(FsError::AlreadyExistsFile),
            };
        }

        let new_id = self.nodes.len();
        self.nodes.push(Node::File {
            name: name.to_string(),
            data: Vec::new(),
            parent: Some(dir),
        });

        let children = self.get_dir_children_mut(dir)?;
        children.insert(name.to_string(), new_id);

        Ok(new_id)
    }

    fn append(&mut self, file: Self::NodeId, bytes: &[u8]) -> Result<(), FsError> {
        let node = self.nodes.get_mut(file).ok_or(FsError::NotFound)?;
        match node {
            Node::File { data, .. } => {
                data.extend_from_slice(bytes);
                Ok(())
            }
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    fn read(&self, file: Self::NodeId) -> Result<Vec<u8>, FsError> {
        let node = self.nodes.get(file).ok_or(FsError::NotFound)?;
        match node {
            Node::File { data, .. } => Ok(data.as_slice().to_owned()),
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    fn write_trunc(&mut self, file: Self::NodeId, bytes: &[u8]) -> Result<usize, FsError> {
        let node = self.nodes.get_mut(file).ok_or(FsError::NotFound)?;
        match node {
            Node::File { data, .. } => {
                data.clear();
                data.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    fn file_read_borrow(&self, file: Self::NodeId) -> Result<&[u8], FsError> {
        let node = self.nodes.get(file).ok_or(FsError::NotFound)?;
        match node {
            Node::File { data, .. } => Ok(data.as_slice()),
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    fn len(&self, file: Self::NodeId) -> Result<usize, FsError> {
        let node = self.nodes.get(file).ok_or(FsError::NotFound)?;
        match node {
            Node::File { data, .. } => Ok(data.len()),
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }

    fn write_at(&mut self, file: Self::NodeId, off: usize, bytes: &[u8]) -> Result<usize, FsError> {
        let node = self.nodes.get_mut(file).ok_or(FsError::NotFound)?;
        match node {
            Node::File { data, .. } => {
                if off > data.len() {
                    data.resize(off, 0);
                }

                let end = off.saturating_add(bytes.len());

                if end > data.len() {
                    data.resize(end, 0);
                }

                data[off..end].copy_from_slice(bytes);

                Ok(bytes.len())
            }
            Node::Dir { .. } => Err(FsError::IsADirectory),
        }
    }
}
