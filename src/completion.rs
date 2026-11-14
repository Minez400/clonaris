//! # Shell Autocompletion Generator
//!
//! Generates native completion scripts for Bash and Zsh shells with zero external dependencies.
//! Provides dynamic autocompletion for commands, options, file paths, and branch names.

pub fn generate_completion(shell: &str) {
    match shell {
        "zsh" => {
            print!("{}", r#"#compdef clonaris

_clonaris() {
    local -a commands
    commands=(
        'init:Initialize a new repository (.clonaris/)'
        'status:Show working tree status (new, modified, deleted)'
        'diff:Show line-by-line colored diffs'
        'save:Create an atomic snapshot directly without staging'
        'amend:Safely update the latest snapshot'
        'discard:Discard uncommitted modifications in files'
        'resolve:Interactively resolve merge conflict markers'
        'log:Display snapshot history with ASCII tree graph'
        'ops:Display chronological transaction log (OpLog)'
        'undo:Safely revert the latest operation'
        'redo:Reapply the latest undone operation'
        'branch:List or create branches'
        'switch:Switch active branch with data-loss protection'
        'merge:Merge another branch (with non-blocking conflicts)'
        'tag:List or create static snapshot tags'
        'completion:Generate shell autocompletion script'
        'help:Display help information'
    )

    _arguments -C \
        '1: :->command' \
        '*:: :->args'

    case $state in
        command)
            _describe -t commands 'clonaris command' commands
            ;;
        args)
            case $words[1] in
                switch|merge)
                    if [ -d ".clonaris/refs/heads" ]; then
                        local branches=(${(f)"$(ls .clonaris/refs/heads 2>/dev/null)"})
                        _values 'branches' $branches
                    fi
                    ;;
                diff|discard|resolve)
                    _files
                    ;;
            esac
            ;;
    esac
}

compdef _clonaris clonaris
"#);
        }
        _ => {
            // Default to Bash
            print!("{}", r#"# Clonaris bash completion
_clonaris_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="init status diff save amend discard resolve log ops undo redo branch switch merge tag completion help"

    case "${prev}" in
        switch|merge)
            if [ -d ".clonaris/refs/heads" ]; then
                local branches=$(ls .clonaris/refs/heads 2>/dev/null)
                COMPREPLY=( $(compgen -W "${branches}" -- ${cur}) )
                return 0
            fi
            ;;
        diff|discard|resolve)
            COMPREPLY=( $(compgen -f -- ${cur}) )
            return 0
            ;;
        *)
            ;;
    esac

    COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
    return 0
}
complete -F _clonaris_completions clonaris
"#);
        }
    }
}
