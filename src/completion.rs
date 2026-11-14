pub fn generate_completion(shell: &str) {
    match shell {
        "zsh" => {
            print!("{}", r#"#compdef clonaris

_clonaris() {
    local -a commands
    commands=(
        'init:Inicializa um novo repositório (.clonaris/)'
        'status:Mostra o estado atual dos arquivos'
        'diff:Mostra as alterações linha a linha (com cores)'
        'save:Grava um novo snapshot atômico diretamente'
        'amend:Atualiza o snapshot mais recente com segurança'
        'discard:Descarta alterações em um arquivo ou em tudo'
        'resolve:Resolve conflitos de merge interativamente'
        'log:Exibe o histórico de snapshots com árvore ASCII'
        'ops:Exibe a linha do tempo de operações (OpLog)'
        'undo:Desfaz a última operação com segurança'
        'redo:Refaz a última operação desfeita'
        'branch:Lista os ramos ou cria um novo ramo'
        'switch:Alterna entre ramos restaurando os arquivos'
        'merge:Mescla outro ramo (com conflitos não-bloqueantes!)'
        'tag:Lista as tags ou cria uma tag no snapshot atual'
        'completion:Gera script de autocompletar para shell'
        'help:Exibe ajuda'
    )

    _arguments -C \
        '1: :->command' \
        '*:: :->args'

    case $state in
        command)
            _describe -t commands 'comando do clonaris' commands
            ;;
        args)
            case $words[1] in
                switch|merge)
                    if [ -d ".clonaris/refs/heads" ]; then
                        local branches=(${(f)"$(ls .clonaris/refs/heads 2>/dev/null)"})
                        _values 'ramos' $branches
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
