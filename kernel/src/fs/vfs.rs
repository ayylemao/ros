use alloc::{collections::btree_map::BTreeMap, string::String, vec::Vec};
use spin::Once;
use sys::syscall::{errors::FsError, FileKindTag};

use crate::{
    fs::{file_descriptor::OpenFile, procfs::Procfs, ramfs::Ramfs, DirEntry, FsBackend},
    utils::irq_lock::IrqMutex,
};

pub type MountId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfsNode {
    pub mount: MountId,
    pub node: usize,
}

pub enum Backend {
    Ramfs(&'static IrqMutex<Ramfs>),
    Procfs(&'static IrqMutex<Procfs>),
}

pub struct Mount {
    pub backend: Backend,
    pub root_node: usize,
    pub mountpoint: Option<VfsNode>,
}

pub struct Vfs {
    mounts: Vec<Mount>,
    by_mountpoint: BTreeMap<(MountId, usize), MountId>,
}

static VFS: Once<IrqMutex<Vfs>> = Once::new();

impl Vfs {
    fn new() -> Self {
        let ramfs = Ramfs::ramfs();
        let root_node = {
            let mut ramfs_lock = ramfs.lock();
            _ = &ramfs_lock.load_initrd();
            ramfs_lock.mkdir_p("/proc").unwrap();
            ramfs_lock.root()
        };

        let mut mounts = Vec::new();
        mounts.push(Mount {
            backend: Backend::Ramfs(ramfs),
            root_node,
            mountpoint: None,
        });

        let procfs = Procfs::get();
        let procfs_root = { procfs.lock().root() };
        let proc_mpoint_node = {
            let r = Ramfs::ramfs().lock();
            r.lookup(r.root(), "proc").unwrap()
        };

        let proc_mpoint = VfsNode {
            mount: 0,
            node: proc_mpoint_node,
        };

        let mut vfs = Self {
            mounts,
            by_mountpoint: BTreeMap::new(),
        };
        vfs.mount_at(proc_mpoint, Backend::Procfs(procfs), procfs_root)
            .unwrap();
        vfs
    }

    pub fn mount_at(
        &mut self,
        mountpoint: VfsNode,
        backend: Backend,
        backend_root: usize,
    ) -> Result<MountId, FsError> {
        if self.backend_node_type(mountpoint.mount, mountpoint.node)? != FileKindTag::Dir {
            return Err(FsError::NotADirectory);
        }

        let child_mnt = self.mounts.len() as u32;

        self.mounts.push(Mount {
            backend,
            root_node: backend_root,
            mountpoint: Some(mountpoint),
        });

        self.by_mountpoint
            .insert((mountpoint.mount, mountpoint.node), child_mnt);
        Ok(child_mnt)
    }

    pub fn get() -> &'static IrqMutex<Vfs> {
        VFS.call_once(|| IrqMutex::new(Vfs::new()))
    }

    fn backend_node_type(&self, mnt: MountId, id: usize) -> Result<FileKindTag, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().node_type(id),
            Backend::Procfs(r) => r.lock().node_type(id),
        }
    }

    fn backend_lookup(&self, mnt: MountId, dir: usize, name: &str) -> Result<usize, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().lookup(dir, name),
            Backend::Procfs(r) => r.lock().lookup(dir, name),
        }
    }

    fn backend_parent(&self, mnt: MountId, id: usize) -> Result<Option<usize>, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().parent(id),
            Backend::Procfs(r) => r.lock().parent(id),
        }
    }

    fn backend_readdir(&self, mnt: MountId, dir: usize) -> Result<Vec<DirEntry>, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().readdir(dir),
            Backend::Procfs(r) => r.lock().readdir(dir),
        }
    }

    fn backend_mkdir(&self, mnt: MountId, dir: usize, name: &str) -> Result<usize, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().mkdir(dir, name),
            Backend::Procfs(r) => r.lock().mkdir(dir, name),
        }
    }

    fn backend_create(&self, mnt: MountId, dir: usize, name: &str) -> Result<usize, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().create(dir, name),
            Backend::Procfs(r) => r.lock().create(dir, name),
        }
    }

    fn backend_read(&self, mnt: MountId, file: usize) -> Result<alloc::vec::Vec<u8>, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().read(file),
            Backend::Procfs(r) => r.lock().read(file),
        }
    }

    fn backend_file_len(&self, mnt: MountId, file: usize) -> Result<usize, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().len(file),
            Backend::Procfs(r) => r.lock().len(file),
        }
    }

    fn backend_write_trunc(
        &self,
        mnt: MountId,
        file: usize,
        bytes: &[u8],
    ) -> Result<usize, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().write_trunc(file, bytes),
            Backend::Procfs(r) => r.lock().write_trunc(file, bytes),
        }
    }

    fn backend_write_at(
        &self,
        mnt: MountId,
        file: usize,
        off: usize,
        bytes: &[u8],
    ) -> Result<usize, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().write_at(file, off, bytes),
            Backend::Procfs(r) => r.lock().write_at(file, off, bytes),
        }
    }

    fn backend_append(&self, mnt: MountId, file: usize, bytes: &[u8]) -> Result<(), FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().append(file, bytes),
            Backend::Procfs(r) => r.lock().append(file, bytes),
        }
    }

    fn mounted_over(&self, at: VfsNode) -> Option<MountId> {
        self.by_mountpoint.get(&(at.mount, at.node)).copied()
    }

    pub fn root_vnode(&self) -> VfsNode {
        VfsNode {
            mount: 0,
            node: self.mounts[0].root_node,
        }
    }

    fn step_lookup(&self, curr: VfsNode, name: &str) -> Result<VfsNode, FsError> {
        let child = self.backend_lookup(curr.mount, curr.node, name)?;
        let mut next = VfsNode {
            mount: curr.mount,
            node: child,
        };

        if let Some(child_mount) = self.mounted_over(next) {
            let root = self.mounts[child_mount as usize].root_node;
            next = VfsNode {
                mount: child_mount,
                node: root,
            }
        }

        Ok(next)
    }

    fn step_parent(&self, curr: VfsNode) -> Result<VfsNode, FsError> {
        let m = &self.mounts[curr.mount as usize];

        if curr.node == m.root_node {
            if let Some(mp) = m.mountpoint {
                let parent = self.backend_parent(mp.mount, mp.node)?.unwrap();
                return Ok(VfsNode {
                    mount: mp.mount,
                    node: parent,
                });
            }
            return Ok(curr);
        }

        let p = self
            .backend_parent(curr.mount, curr.node)?
            .unwrap_or(m.root_node);

        Ok(VfsNode {
            mount: curr.mount,
            node: p,
        })
    }

    fn backend_name<'a>(&'a self, mnt: MountId, id: usize) -> Result<String, FsError> {
        match &self.mounts[mnt as usize].backend {
            Backend::Ramfs(r) => r.lock().name(id),
            Backend::Procfs(r) => r.lock().name(id),
        }
    }

    pub fn vnode_to_path(&self, mut node: VfsNode) -> Result<String, FsError> {
        if node == self.root_vnode() {
            return Ok(String::from("/"));
        }

        let mut parts: Vec<String> = Vec::new();

        loop {
            if node == self.root_vnode() {
                break;
            }

            let m = &self.mounts[node.mount as usize];
            if node.node == m.root_node {
                if let Some(mp) = m.mountpoint {
                    node = mp;
                    continue;
                }
            }

            let name = self.backend_name(node.mount, node.node)?;
            parts.push(name);

            let next = self.step_parent(node)?;

            if next == node {
                break;
            }
            node = next;
        }

        parts.reverse();

        let mut out = String::from("/");
        for (i, p) in parts.iter().enumerate() {
            if i != 0 {
                out.push('/');
            }
            out.push_str(p);
        }
        Ok(out)
    }

    pub fn resolve(&self, start: VfsNode, path: &str) -> Result<VfsNode, FsError> {
        let mut curr = if path.starts_with("/") {
            self.root_vnode()
        } else {
            start
        };

        for comp in path.split('/').filter(|c| !c.is_empty()) {
            match comp {
                "." => {}
                ".." => {
                    curr = self.step_parent(curr)?;
                }
                name => {
                    curr = self.step_lookup(curr, name)?;
                }
            }
        }

        Ok(curr)
    }

    pub fn resolve_parent<'a>(
        &self,
        start: VfsNode,
        path: &'a str,
    ) -> Result<(VfsNode, &'a str), FsError> {
        let p = path.trim_end_matches('/');
        if p.is_empty() || p == "/" {
            return Err(FsError::InvalidPath);
        }

        let (parent_part, leaf) = match p.rsplit_once('/') {
            Some(v) => v,
            None => {
                if p == "." || p == ".." {
                    return Err(FsError::InvalidPath);
                }
                let parent = if path.starts_with('/') {
                    self.root_vnode()
                } else {
                    start
                };
                return Ok((parent, p));
            }
        };

        if leaf.is_empty() || leaf == "." || leaf == ".." {
            return Err(FsError::InvalidPath);
        }

        let parent_path = if parent_part.is_empty() {
            "/"
        } else {
            parent_part
        };
        let parent_vnode = self.resolve(start, parent_path)?;
        Ok((parent_vnode, leaf))
    }

    pub fn readdir_path(&self, cwd: VfsNode, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let n = self.resolve(cwd, path)?;
        self.backend_readdir(n.mount, n.node)
    }

    pub fn readdir_node(&self, dir: VfsNode) -> Result<Vec<DirEntry>, FsError> {
        self.backend_readdir(dir.mount, dir.node)
    }

    pub fn create_path(&self, cwd: VfsNode, path: &str) -> Result<VfsNode, FsError> {
        let (parent, leaf) = self.resolve_parent(cwd, path)?;
        let id = self.backend_create(parent.mount, parent.node, leaf)?;
        Ok(VfsNode {
            mount: parent.mount,
            node: id,
        })
    }

    pub fn mkdir_path(&self, cwd: VfsNode, path: &str) -> Result<VfsNode, FsError> {
        let (parent, leaf) = self.resolve_parent(cwd, path)?;
        let id = self.backend_mkdir(parent.mount, parent.node, leaf)?;
        Ok(VfsNode {
            mount: parent.mount,
            node: id,
        })
    }

    pub fn node_type(&self, node: VfsNode) -> Result<FileKindTag, FsError> {
        self.backend_node_type(node.mount, node.node)
    }

    pub fn read_at(
        &self,
        node: VfsNode,
        openfile: &mut OpenFile,
        len: usize,
    ) -> Result<Vec<u8>, FsError> {
        if openfile.cache.is_none() {
            openfile.cache = Some(self.backend_read(node.mount, node.node)?);
        }
        let data = openfile.cache.as_ref().unwrap();
        if openfile.offset >= data.len() {
            return Ok(Vec::new());
        }
        let n = core::cmp::min(len, data.len() - openfile.offset);
        Ok(data[openfile.offset..openfile.offset + n].to_vec())
    }

    pub fn read_all(&self, node: VfsNode) -> Result<Vec<u8>, FsError> {
        self.backend_read(node.mount, node.node)
    }

    pub fn write_at(&self, node: VfsNode, off: usize, bytes: &[u8]) -> Result<usize, FsError> {
        self.backend_write_at(node.mount, node.node, off, bytes)
    }

    pub fn file_len(&self, node: VfsNode) -> Result<usize, FsError> {
        self.backend_file_len(node.mount, node.node)
    }
}
