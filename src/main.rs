mod commands;
mod diff;
mod hash;
mod objects;
mod oplog;
mod repository;
mod workspace;

use std::env;

fn main() {
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
                eprintln!("Erro: O comando save requer uma mensagem. Ex: clonaris save -m \"Alterações\"");
                std::process::exit(1);
            }
            commands::cmd_save(&message)
        }
        "log" => commands::cmd_log(),
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
                eprintln!("Erro: Especifique o ramo para o qual deseja alternar. Ex: clonaris switch feature");
                std::process::exit(1);
            }
            commands::cmd_switch(&args[2])
        }
        "tag" => {
            let tag_name = if args.len() > 2 {
                Some(args[2].as_str())
            } else {
                None
            };
            commands::cmd_tag(tag_name)
        }
        "help" | "--help" | "-h" => {
            commands::print_help();
            Ok(())
        }
        unknown => {
            eprintln!("Comando desconhecido: '{}'", unknown);
            println!("Execute 'clonaris help' para ver os comandos disponíveis.");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("\x1b[1;31mErro fatal:\x1b[0m {}", e);
        std::process::exit(1);
    }
}
