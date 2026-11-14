use std::env;
use std::fs;
use std::io;

use crate::diff::{compute_diff, format_colored_diff, is_binary};
use crate::objects::{parse_commit, read_object};
use crate::oplog::{
    append_op, pop_last_op, pop_last_redo, push_redo, restore_op_to_log,
};
use crate::repository::{
    create_tag, find_repo_root, get_current_branch, get_head, init, list_tags, set_head,
};
use crate::workspace::{get_head_tree, get_status, restore_tree, save};

pub fn cmd_init() -> io::Result<()> {
    let current = env::current_dir()?;
    init(&current)?;
    println!("✓ Repositório Clonaris inicializado com sucesso em {}", current.display());
    println!("  Ramo inicial: main");
    println!("  Dica: crie seus arquivos e execute 'clonaris save -m \"sua mensagem\"'");
    Ok(())
}

pub fn cmd_status() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado neste diretório ou superiores.");
            std::process::exit(1);
        }
    };

    let branch = get_current_branch(&root);
    let head = get_head(&root)?;
    let diff = get_status(&root)?;

    println!("No ramo: \x1b[1;32m{}\x1b[0m", branch);
    match head {
        Some(h) => println!("Último snapshot: \x1b[33m{:.8}\x1b[0m", h),
        None => println!("Nenhum snapshot salvo ainda."),
    }
    println!();

    if diff.is_clean() {
        println!("✓ Diretório de trabalho limpo. Nenhuma alteração pendente.");
        return Ok(());
    }

    if !diff.added.is_empty() {
        println!("\x1b[1;32mNovos arquivos:\x1b[0m");
        for file in &diff.added {
            println!("  + {}", file);
        }
    }

    if !diff.modified.is_empty() {
        println!("\x1b[1;34mArquivos modificados:\x1b[0m");
        for file in &diff.modified {
            println!("  ~ {}", file);
        }
    }

    if !diff.deleted.is_empty() {
        println!("\x1b[1;31mArquivos deletados:\x1b[0m");
        for file in &diff.deleted {
            println!("  - {}", file);
        }
    }

    println!();
    println!("Execute 'clonaris diff' para ver as alterações linha a linha.");
    println!("Execute 'clonaris save -m \"mensagem\"' para gravar o snapshot diretamente.");
    Ok(())
}

pub fn cmd_diff(target_file: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let diff = get_status(&root)?;
    let head_tree = get_head_tree(&root)?;

    if diff.is_clean() {
        println!("Nenhuma alteração para exibir diff.");
        return Ok(());
    }

    let mut printed_anything = false;

    // Modified files
    for file in &diff.modified {
        if let Some(target) = target_file {
            if file != target {
                continue;
            }
        }

        printed_anything = true;
        let old_hash = head_tree.get(file).unwrap();
        let old_bytes = read_object(&root, old_hash)?;
        let new_bytes = fs::read(root.join(file))?;

        if is_binary(&old_bytes) || is_binary(&new_bytes) {
            println!("\x1b[1;33mArquivo binário {} difere do snapshot anterior\x1b[0m", file);
            continue;
        }

        let old_text = String::from_utf8_lossy(&old_bytes);
        let new_text = String::from_utf8_lossy(&new_bytes);

        let lines = compute_diff(&old_text, &new_text);
        print!("{}", format_colored_diff(file, &lines));
    }

    // Added files
    for file in &diff.added {
        if let Some(target) = target_file {
            if file != target {
                continue;
            }
        }

        printed_anything = true;
        let new_bytes = fs::read(root.join(file))?;
        if is_binary(&new_bytes) {
            println!("\x1b[1;32mNovo arquivo binário adicionado: {}\x1b[0m", file);
            continue;
        }

        let new_text = String::from_utf8_lossy(&new_bytes);
        let lines = compute_diff("", &new_text);
        print!("{}", format_colored_diff(file, &lines));
    }

    // Deleted files
    for file in &diff.deleted {
        if let Some(target) = target_file {
            if file != target {
                continue;
            }
        }

        printed_anything = true;
        let old_hash = head_tree.get(file).unwrap();
        let old_bytes = read_object(&root, old_hash)?;
        if is_binary(&old_bytes) {
            println!("\x1b[1;31mArquivo binário excluído: {}\x1b[0m", file);
            continue;
        }

        let old_text = String::from_utf8_lossy(&old_bytes);
        let lines = compute_diff(&old_text, "");
        print!("{}", format_colored_diff(file, &lines));
    }

    if !printed_anything && target_file.is_some() {
        println!("O arquivo '{}' não possui alterações pendentes.", target_file.unwrap());
    }

    Ok(())
}

pub fn cmd_save(message: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let diff = get_status(&root)?;
    if diff.is_clean() {
        println!("Nada para salvar (diretório de trabalho limpo).");
        return Ok(());
    }

    let total_changes = diff.added.len() + diff.modified.len() + diff.deleted.len();
    match save(&root, message)? {
        Some(commit_hash) => {
            let branch = get_current_branch(&root);
            println!(
                "✓ [\x1b[1;32m{}\x1b[0m \x1b[33m{:.8}\x1b[0m] {}",
                branch, commit_hash, message
            );
            println!(
                "  {} alteração(ões): +{} novo(s), ~{} modificado(s), -{} deletado(s)",
                total_changes,
                diff.added.len(),
                diff.modified.len(),
                diff.deleted.len()
            );
        }
        None => {
            println!("Nada para salvar.");
        }
    }
    Ok(())
}

pub fn cmd_log() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let mut current_hash = match get_head(&root)? {
        Some(h) => h,
        None => {
            println!("Nenhum snapshot encontrado neste repositório.");
            return Ok(());
        }
    };

    let mut count = 0;
    while !current_hash.is_empty() {
        let commit_data = match read_object(&root, &current_hash) {
            Ok(d) => d,
            Err(_) => break,
        };

        let commit = match parse_commit(&commit_data) {
            Ok(c) => c,
            Err(_) => break,
        };

        let date_str = format_timestamp(commit.timestamp);

        println!("\x1b[33m*   commit {}\x1b[0m", current_hash);
        println!("|   Autor:     {}", commit.author);
        println!("|   Data:      {}", date_str);
        println!("|   Árvore:    {:.8}", commit.tree);
        println!("|");
        for line in commit.message.lines() {
            println!("|       {}", line);
        }
        println!("|");

        count += 1;
        if let Some(parent) = commit.parents.first() {
            current_hash = parent.clone();
        } else {
            break;
        }
    }

    println!("Total de snapshots: {}", count);
    Ok(())
}

pub fn cmd_undo() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let last_op = match pop_last_op(&root)? {
        Some(op) => op,
        None => {
            println!("Nenhuma operação no histórico para desfazer.");
            return Ok(());
        }
    };

    println!(
        "Desfazendo operação: \x1b[1;36m{}\x1b[0m ({})",
        last_op.action, last_op.description
    );

    // Push into redo stack before reverting
    push_redo(&root, &last_op)?;

    if last_op.prev_head == "none" {
        // Revert to pristine state (before first commit)
        let head_ref = root.join(".clonaris").join("refs").join("heads").join("main");
        let _ = fs::remove_file(head_ref);
        println!("✓ Retornado ao estado inicial (repositório sem commits).");
    } else {
        // Restore prev_head commit
        let commit_data = read_object(&root, &last_op.prev_head)?;
        let commit = parse_commit(&commit_data)?;

        restore_tree(&root, &commit.tree)?;
        set_head(&root, &last_op.prev_head)?;

        println!(
            "✓ Snapshot restaurado com sucesso para \x1b[33m{:.8}\x1b[0m",
            last_op.prev_head
        );
    }

    println!("Dica: use 'clonaris redo' se quiser refazer esta alteração.");
    Ok(())
}

pub fn cmd_redo() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let redo_op = match pop_last_redo(&root)? {
        Some(op) => op,
        None => {
            println!("Nenhuma operação desfeita para refazer.");
            return Ok(());
        }
    };

    println!(
        "Refazendo operação: \x1b[1;36m{}\x1b[0m ({})",
        redo_op.action, redo_op.description
    );

    if redo_op.new_head != "none" {
        let commit_data = read_object(&root, &redo_op.new_head)?;
        let commit = parse_commit(&commit_data)?;

        restore_tree(&root, &commit.tree)?;
        set_head(&root, &redo_op.new_head)?;

        // Put op back into oplog
        restore_op_to_log(&root, &redo_op)?;

        println!(
            "✓ Snapshot restabelecido para \x1b[33m{:.8}\x1b[0m",
            redo_op.new_head
        );
    }

    Ok(())
}

pub fn cmd_branch(name: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let heads_dir = root.join(".clonaris").join("refs").join("heads");

    match name {
        Some(new_branch) => {
            let branch_file = heads_dir.join(new_branch);
            if branch_file.exists() {
                eprintln!("Erro: O ramo '{}' já existe.", new_branch);
                return Ok(());
            }

            let current_head = match get_head(&root)? {
                Some(h) => h,
                None => {
                    eprintln!("Erro: Não é possível criar um ramo sem ao menos um snapshot salvo.");
                    return Ok(());
                }
            };

            fs::write(branch_file, format!("{}\n", current_head))?;
            println!("✓ Ramo '\x1b[1;32m{}\x1b[0m' criado no snapshot {:.8}", new_branch, current_head);
        }
        None => {
            let current = get_current_branch(&root);
            println!("Ramos disponíveis:");
            if let Ok(entries) = fs::read_dir(heads_dir) {
                for entry in entries.flatten() {
                    let b_name = entry.file_name().to_string_lossy().to_string();
                    if b_name == current {
                        println!("* \x1b[1;32m{}\x1b[0m (atual)", b_name);
                    } else {
                        println!("  {}", b_name);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn cmd_switch(branch_name: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    let branch_file = root.join(".clonaris").join("refs").join("heads").join(branch_name);
    if !branch_file.exists() {
        eprintln!("Erro: O ramo '{}' não foi encontrado.", branch_name);
        return Ok(());
    }

    let target_commit_hash = fs::read_to_string(&branch_file)?.trim().to_string();
    let commit_data = read_object(&root, &target_commit_hash)?;
    let commit = parse_commit(&commit_data)?;

    // Restore files
    restore_tree(&root, &commit.tree)?;

    // Update HEAD symbolic ref
    let head_file = root.join(".clonaris").join("HEAD");
    fs::write(head_file, format!("ref: refs/heads/{}\n", branch_name))?;

    // Record op
    append_op(
        &root,
        "switch",
        None,
        Some(&target_commit_hash),
        &format!("Alternou para ramo {}", branch_name),
    )?;

    println!("✓ Alternou para o ramo '\x1b[1;32m{}\x1b[0m' ({:.8})", branch_name, target_commit_hash);
    Ok(())
}

pub fn cmd_tag(name: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Erro: Nenhum repositório Clonaris encontrado.");
            std::process::exit(1);
        }
    };

    match name {
        Some(tag_name) => {
            let current_head = match get_head(&root)? {
                Some(h) => h,
                None => {
                    eprintln!("Erro: Não é possível criar uma tag sem commits.");
                    return Ok(());
                }
            };
            create_tag(&root, tag_name, &current_head)?;
            println!("✓ Tag '\x1b[1;35m{}\x1b[0m' criada no snapshot {:.8}", tag_name, current_head);
        }
        None => {
            let tags = list_tags(&root)?;
            if tags.is_empty() {
                println!("Nenhuma tag criada ainda. Use 'clonaris tag <nome>' para criar uma.");
                return Ok(());
            }
            println!("Tags:");
            for (tag, hash) in tags {
                println!("  \x1b[1;35m{:<15}\x1b[0m -> \x1b[33m{:.8}\x1b[0m", tag, hash);
            }
        }
    }
    Ok(())
}

pub fn print_help() {
    println!("\x1b[1mClonaris VCS\x1b[0m - Controle de versão simples, direto e sem atrito");
    println!("100% Rust Puro (Zero dependências externas)");
    println!();
    println!("\x1b[1mUSO:\x1b[0m");
    println!("    clonaris <COMANDO> [OPÇÕES]");
    println!();
    println!("\x1b[1mCOMANDOS:\x1b[0m");
    println!("    init                 Inicializa um novo repositório (.clonaris/)");
    println!("    status               Mostra o estado atual dos arquivos");
    println!("    diff [arquivo]       Mostra as alterações linha a linha (com cores)");
    println!("    save -m \"<msg>\"      Grava um novo snapshot atômico diretamente");
    println!("    log                  Exibe o histórico de snapshots com árvore ASCII");
    println!("    undo                 Desfaz a última operação com segurança");
    println!("    redo                 Refaz a última operação desfeita");
    println!("    branch [nome]        Lista os ramos ou cria um novo ramo");
    println!("    switch <ramo>        Alterna para outro ramo restaurando os arquivos");
    println!("    tag [nome]           Lista as tags ou cria uma tag no snapshot atual");
    println!("    help                 Exibe esta ajuda");
    println!();
    println!("\x1b[1mEXEMPLO:\x1b[0m");
    println!("    clonaris init");
    println!("    echo 'hello' > index.txt");
    println!("    clonaris save -m 'Primeiro commit'");
    println!("    echo 'world' >> index.txt");
    println!("    clonaris diff");
    println!("    clonaris undo");
    println!("    clonaris redo");
}

fn format_timestamp(timestamp: u64) -> String {
    let seconds_in_day = 86400;
    let days = timestamp / seconds_in_day;
    let rem_seconds = timestamp % seconds_in_day;
    let hours = rem_seconds / 3600;
    let minutes = (rem_seconds % 3600) / 60;
    let seconds = rem_seconds % 60;

    let mut year = 1970;
    let mut d = days;

    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        year += 1;
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30,
        31, 31, 30, 31, 30, 31,
    ];

    let mut month = 1;
    for &dim in &days_in_months {
        if d < dim {
            break;
        }
        d -= dim;
        month += 1;
    }
    let day = d + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hours, minutes, seconds
    )
}
