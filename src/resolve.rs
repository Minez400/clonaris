use std::fs;
use std::io::{self, stdin, stdout, Write};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveChoice {
    Current,
    Target,
}

pub fn has_conflicts(content: &str) -> bool {
    content.contains("<<<<<<< ATUAL") && content.contains("=======") && content.contains(">>>>>>> RAMO ALVO")
}

pub fn resolve_conflict_markers(content: &str, choice: ResolveChoice) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].starts_with("<<<<<<< ATUAL") {
            let mut current_block = Vec::new();
            let mut target_block = Vec::new();
            let mut in_target = false;
            i += 1;

            while i < lines.len() && !lines[i].starts_with(">>>>>>> RAMO ALVO") {
                if lines[i] == "=======" {
                    in_target = true;
                } else if in_target {
                    target_block.push(lines[i]);
                } else {
                    current_block.push(lines[i]);
                }
                i += 1;
            }

            if i < lines.len() && lines[i].starts_with(">>>>>>> RAMO ALVO") {
                i += 1; // Skip the closing marker
            }

            match choice {
                ResolveChoice::Current => {
                    for line in current_block {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                ResolveChoice::Target => {
                    for line in target_block {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        } else {
            out.push_str(lines[i]);
            out.push('\n');
            i += 1;
        }
    }

    out
}

pub fn prompt_resolve_file(file_path: &Path, strategy_arg: Option<&str>) -> io::Result<()> {
    if !file_path.exists() {
        eprintln!("Erro: Arquivo '{}' não encontrado.", file_path.display());
        return Ok(());
    }

    let content = fs::read_to_string(file_path)?;
    if !has_conflicts(&content) {
        println!("✓ O arquivo '{}' não possui conflitos pendentes.", file_path.display());
        return Ok(());
    }

    let choice = match strategy_arg {
        Some("--current") | Some("-c") => ResolveChoice::Current,
        Some("--target") | Some("-t") => ResolveChoice::Target,
        _ => {
            println!("\x1b[1mConflito detectado em:\x1b[0m \x1b[33m{}\x1b[0m", file_path.display());
            println!("Como deseja resolver este conflito?");
            println!("  [1] Manter versão atual (minha)");
            println!("  [2] Aceitar versão do ramo mesclado (deles)");
            println!("  [3] Cancelar");
            print!("Escolha [1/2/3]: ");
            stdout().flush()?;

            let mut input = String::new();
            stdin().read_line(&mut input)?;
            match input.trim() {
                "1" => ResolveChoice::Current,
                "2" => ResolveChoice::Target,
                _ => {
                    println!("Resolução cancelada.");
                    return Ok(());
                }
            }
        }
    };

    let resolved = resolve_conflict_markers(&content, choice);
    fs::write(file_path, resolved)?;

    println!("✓ Conflitos resolvidos com sucesso em '{}'!", file_path.display());
    println!("  Dica: salve a resolução com: clonaris save -m \"Resolve conflito em {}\"", file_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_current() {
        let text = "antes\n<<<<<<< ATUAL (main)\nminha linha\n=======\nlinha deles\n>>>>>>> RAMO ALVO (feature)\ndepois\n";
        let res = resolve_conflict_markers(text, ResolveChoice::Current);
        assert_eq!(res, "antes\nminha linha\ndepois\n");
    }

    #[test]
    fn test_resolve_target() {
        let text = "antes\n<<<<<<< ATUAL (main)\nminha linha\n=======\nlinha deles\n>>>>>>> RAMO ALVO (feature)\ndepois\n";
        let res = resolve_conflict_markers(text, ResolveChoice::Target);
        assert_eq!(res, "antes\nlinha deles\ndepois\n");
    }
}
