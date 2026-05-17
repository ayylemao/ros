use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use sys::{
    FileOpenFlags, print, println,
    syscall::{
        self,
        errors::Errno,
        wrappers::{chdir, exit, waitpid},
    },
};
use user_rt::{fs::file::File, process};

pub struct Shell {
    cwd: String,
    path: Vec<String>,
    history: File,
}

impl Shell {
    pub fn new() -> Self {
        let mut path = Vec::new();
        path.push("/usr/bin".to_string());
        //path.push("/usr/scripts".to_string()); # TODO: Add fstat to check for file existining or on startup add hashmap of existing files
        let history = File::open(
            ".history",
            FileOpenFlags::CREATE
                | FileOpenFlags::READ
                | FileOpenFlags::WRITE
                | FileOpenFlags::APPEND,
        )
        .unwrap();
        Shell {
            cwd: "/".to_string(),
            path,
            history,
        }
    }

    pub fn prompt(&self) {
        print!("ros:{}$ ", self.cwd)
    }

    pub fn run_line(&mut self, line: &str) -> Result<(), Errno> {
        self.history.write(line.to_string())?;
        let line = line.split('\0').next().unwrap_or("").trim();
        if line.is_empty() {
            return Ok(());
        }

        let args: Vec<&str> = line.split_whitespace().collect();
        let Some(&name) = args.get(0) else {
            return Ok(());
        };

        let rest = &args[1..];

        let nohup = if let Some(last_arg) = rest.last() {
            if *last_arg == "&" { true } else { false }
        } else {
            false
        };

        match name {
            "cd" => {
                let Some(path) = args.get(1).copied() else {
                    self.cwd.clear();
                    self.cwd.push_str("/");
                    chdir("/")?;
                    return Ok(());
                };
                let res = match chdir(path) {
                    Ok(v) => v,
                    Err(e) => {
                        println!("cd: {path}: {:?}", e);
                        return Ok(());
                    }
                };
                self.cwd = user_rt::env::get_cwd().unwrap();
            }
            "exit" => {
                println!("exit");
                exit(0);
            }
            "ps" => {
                _ = syscall::wrappers::ps();
            }
            _ => {
                if let Some(resolved_cmd) = self.resolve_command(name) {
                    let shell_script = match Self::check_for_shell_script(&resolved_cmd) {
                        Ok(v) => v,
                        Err(e) => None,
                    };
                    if let Some(shell_script) = shell_script {
                        self.run_shell_script(shell_script)?;
                        return Ok(());
                    }

                    if nohup {
                        match process::spawn(&resolved_cmd, &rest[..rest.len() - 1]) {
                            Ok(v) => {}
                            Err(e) => println!("{}: failed with errno {:?}", name, e),
                        }
                    } else {
                        match process::spawn(&resolved_cmd, rest) {
                            Ok(v) => match waitpid(v as u64) {
                                Ok(code) => {}
                                Err(e) => println!("waitpid failed: {:?}", e),
                            },
                            Err(e) => println!("{}: failed with errno {:?}", name, e),
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn resolve_command(&self, cmd: &str) -> Option<String> {
        if cmd.contains('/') {
            // absolute or relative path
            return Some(self.normalize_path(cmd));
        }

        // bare command: search PATH
        for dir in &self.path {
            let candidate = format!("{}/{}", dir, cmd);
            //if file_exists_and_executable(&candidate) {
            //    return Some(candidate);
            //}
            return Some(candidate);
        }

        None
    }

    pub fn normalize_path(&self, path: &str) -> String {
        let mut components: Vec<&str> = Vec::new();

        if path.starts_with('/') {
            // absolute path → start from root
        } else {
            // relative path → start from cwd
            for part in self.cwd.split('/') {
                if !part.is_empty() {
                    components.push(part);
                }
            }
        }

        for part in path.split('/') {
            match part {
                "" | "." => {
                    // skip
                }
                ".." => {
                    components.pop();
                }
                _ => {
                    components.push(part);
                }
            }
        }

        let mut result = String::from("/");
        result.push_str(&components.join("/"));
        result
    }

    fn check_for_shell_script(resolved_cmd: &str) -> Result<Option<Vec<u8>>, Errno> {
        let f = File::open(resolved_cmd, FileOpenFlags::READ)?;

        let contents_u8 = f.read()?;
        f.close()?;
        if contents_u8.starts_with("#!shell".as_bytes()) {
            return Ok(Some(contents_u8));
        }
        Ok(None)
    }

    fn run_shell_script(&mut self, contents_u8: Vec<u8>) -> Result<bool, Errno> {
        let content = String::from_utf8(contents_u8).unwrap();
        let lines: Vec<&str> = content.split("\n").collect();
        for line in lines.iter().skip(1) {
            let line = line.split('\0').next().unwrap_or("").trim();
            self.run_line(line)?;
        }
        Ok(false)
    }
}
