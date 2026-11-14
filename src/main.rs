//! # Clonaris VCS - Command Line Interface Dispatcher
//!
//! Main entry point for the Clonaris version control system.
//! Handles command line argument parsing, Unix SIGPIPE resets, and subcommand dispatching.

mod commands;
mod completion;
mod diff;
mod hash;
mod merge;
mod objects;
mod oplog;
mod repository;
mod resolve;
mod workspace;

use std::env;

/// On Unix, resets SIGPIPE signal behavior to default (`SIG_DFL`).
/// This ensures standard pipe behavior when piping output into commands like `head`, `less`, etc.,
/// terminating gracefully without panicking with `Broken pipe (os error 32)`.
#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        unsafe extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        const SIGPIPE: i32 = 13;
        const SIG_DFL: usize = 0;
        signal(SIGPIPE, SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() {
    reset_sigpipe();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        commands::print_help();
        return;
    }

    let command = args[1].as_str();

    let result = match command {
        "init" => commands::cmd_init(),
        "status" => commands::cmd_status(),
        "diff" => {
            let target_file = if args.len() > 2 {
                Some(args[2].as_str())
            } else {
                None
            };
            commands::cmd_diff(target_file)
        }
        "save" => {
            let mut message = String::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "-m" && i + 1 < args.len() {
                    message = args[i + 1].clone();
                    i += 2;
                } else if !args[i].starts_with('-') && message.is_empty() {
                    message = args[i].clone();
                    i += 1;
                } else {
                    i += 1;
                }
            }
            if message.trim().is_empty() {
                eprintln!("Error: The 'save' command requires a message. Example: clonaris save -m \"Changes\"");
                std::process::exit(1);
            }
            commands::cmd_save(&message)
        }
        "amend" => {
            let mut message = String::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "-m" && i + 1 < args.len() {
                    message = args[i + 1].clone();
                    i += 2;
                } else if !args[i].starts_with('-') && message.is_empty() {
                    message = args[i].clone();
                    i += 1;
                } else {
                    i += 1;
                }
            }
            if message.trim().is_empty() {
                eprintln!("Error: The 'amend' command requires a message. Example: clonaris amend -m \"New message\"");
                std::process::exit(1);
            }
            commands::cmd_amend(&message)
        }
        "discard" => {
            if args.len() < 3 {
                eprintln!("Error: Specify the file to discard modifications or '.' for all. Example: clonaris discard main.rs");
                std::process::exit(1);
            }
            commands::cmd_discard(&args[2])
        }
        "resolve" => {
            if args.len() < 3 {
                eprintln!("Error: Specify the file to resolve conflicts. Example: clonaris resolve main.rs [--current/--target]");
                std::process::exit(1);
            }
            let strategy = if args.len() > 3 {
                Some(args[3].as_str())
            } else {
                None
            };
            commands::cmd_resolve(&args[2], strategy)
        }
        "log" => commands::cmd_log(),
        "ops" => commands::cmd_ops(),
        "undo" => commands::cmd_undo(),
        "redo" => commands::cmd_redo(),
        "branch" => {
            let branch_name = if args.len() > 2 {
                Some(args[2].as_str())
            } else {
                None
            };
            commands::cmd_branch(branch_name)
        }
        "switch" => {
            if args.len() < 3 {
                eprintln!("Error: Specify the branch to switch to. Example: clonaris switch feature");
                std::process::exit(1);
            }
            commands::cmd_switch(&args[2])
        }
        "merge" => {
            if args.len() < 3 {
                eprintln!("Error: Specify the branch to merge. Example: clonaris merge feature");
                std::process::exit(1);
            }
            commands::cmd_merge(&args[2])
        }
        "tag" => {
            let tag_name = if args.len() > 2 {
                Some(args[2].as_str())
            } else {
                None
            };
            commands::cmd_tag(tag_name)
        }
        "completion" => {
            let shell = if args.len() > 2 {
                Some(args[2].as_str())
            } else {
                None
            };
            commands::cmd_completion(shell)
        }
        "help" | "--help" | "-h" => {
            commands::print_help();
            Ok(())
        }
        unknown => {
            eprintln!("Unknown command: '{}'", unknown);
            println!("Run 'clonaris help' to see available commands.");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("\x1b[1;31mFatal error:\x1b[0m {}", e);
        std::process::exit(1);
    }
}
