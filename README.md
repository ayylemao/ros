# ros

This repository is a Rust-based operating system technical exploration project.

[![Watch the video](https://raw.githubusercontent.com/ayylemao/ros/main/media/demo.gif)](https://raw.githubusercontent.com/ayylemao/ros/main/media/demo.mp4))

It is an experimental `x86_64` OS with a project-specific UEFI loader, a monolithic kernel, preemptive-ish task scheduling, a small syscall layer, a virtual filesystem, an initramfs, and a primitive but real userspace with an `init` process and shell-like programs. The loader is not a firmware replacement or a full boot ecosystem; it is a Rust UEFI application that uses the UEFI bindings to load the kernel, prepare boot information, set up the initial address space, and jump into the kernel.

The kernel was also an experiment in moving toward a POSIX-like userspace interface. The long-term direction was to support enough of the expected syscall surface and ABI behavior that musl could be used as the system C library. That work was incomplete: many syscalls are missing, partial, or not ABI-compatible with Linux/POSIX expectations, and some interfaces are only implemented well enough for the included test programs.

It also includes minimal "shell scripting" and runs files starting with `!shell` line by line as shell commands.

It is not intended to be a finished or production-ready operating system. It is being open sourced as-is for people who may find the code interesting as a reference, learning resource, or starting point for their own experiments.

## What is here

The project contains a small experimental OS stack targeting `x86_64` and UEFI.

Main components:

- `loader/`  
  A UEFI bootloader written in Rust. It loads the kernel ELF, prepares boot information, sets up initial paging, exits UEFI boot services, and jumps into the kernel.

- `kernel/`  
  The kernel. It includes early initialization, paging, heap setup, framebuffer console output, interrupt setup, APIC/PIT-related code, syscall handling, process and task management, basic scheduling, virtual filesystem pieces, RAM filesystem support, procfs-like structures, and user-mode handoff.

- `shared/`  
  Shared data structures used between the bootloader and kernel, such as boot information and memory region descriptions.

- `sys/`  
  Shared syscall definitions, syscall wrappers, simple stdio-style helpers, constants, and userspace-facing types.

- `user_rt/`  
  A minimal userspace runtime used by the user programs. It provides process startup, heap support, filesystem helpers, and panic handling.

- `user/`  
  Small userspace programs, including `init`, `shell`, `ls`, `cat`, `sched_demo`, and `task_demo`.

- `initrfs/`  
  Initial RAM filesystem contents used by the kernel.

## Building and running

The project uses a Cargo workspace and a `Makefile`.

The default target builds the userspace programs, packages the initramfs, builds the UEFI loader and kernel, installs them into a UEFI disk image, and runs the system in QEMU.

```sh
make
```

The build expects a Rust nightly toolchain compatible with the custom targets used by the project. The `Makefile` currently uses:

```sh
nightly-2025-10-03
```

You will also need typical OS-development tooling such as QEMU, OVMF firmware, FAT image tools, and Rust target/build support for the custom JSON targets.

## External source trees

During development, this working tree also contained experiments involving external projects such as musl and toybox. Those projects are not authored here and are not intended to be part of this repository unless their licenses and source distribution requirements are handled separately.

## Hardware testing

This was tested on a real ThinkPad T440p laptop, where it did boot and run. It also experienced frequent deadlocks on that hardware, which were never investigated or fixed.

## AI-assisted code
I used AI in some areas where i had absolutly no expirience like configuring the APIC to get keyboard interrupts or after days of not getting the round robin scheduler working. Most of the code was written by me but I also used AI to find out some technicals rather than reading Intel/AMD documentation.

## Status

This project is no longer actively developed.

It should be treated as a technical exploration and learning project. Some parts may be incomplete, rough, experimental, or specific to the original development environment.

The code is published so others can inspect it, learn from it, reuse ideas, or continue experimenting with it.
