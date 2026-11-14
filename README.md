# 🪐 Clonaris VCS

**Clonaris** é um sistema de controle de versão (VCS) moderno, ultrarrápido e sem atritos, construído em **100% Rust Puro** com **ZERO dependências externas** (utilizando exclusivamente a biblioteca padrão `std`).

---

## ⚡ Por que Clonaris resolve as maiores dores dos Devs no Git?

1. **Conflitos de Merge Não-Bloqueantes (*First-Class Conflicts*):**
   * *No Git:* O repositório entra em pânico, trava em modo intermediário e impede você de trocar de branch ou executar outros comandos até resolver tudo ou dar abort.
   * *No Clonaris:* O comando `clonaris merge <ramo>` grava o snapshot com ambos os pais e com os marcadores de conflito nos arquivos. **Seu repositório NUNCA fica travado.**
2. **Resolução de Conflitos Inteligente (`clonaris resolve <arquivo>`):**
   * Chega de apagar manualmente marcadores de conflito. Escolha interativamente ou use `--current` ou `--target` para resolver conflitos de forma instantânea.
3. **Descarte Seletivo de Arquivos (`clonaris discard <arquivo>`):**
   * Descarta alterações indesejadas de um arquivo específico ou de todo o diretório (`.`) sem precisar desfazer o snapshot inteiro.
4. **Linha do Tempo Humana (`clonaris ops`):**
   * Esqueça o pesadelo ininteligível do `git reflog`. O Clonaris possui uma esteira numerada com data, hora, ação e descrição legível de cada comando executado.
5. **Viagem no Tempo Totalmente Segura (`undo` e `redo`):**
   * Qualquer ação (`save`, `amend`, `merge`, `switch`) pode ser desfeita e refeita instantaneamente sem risco de perda de código.
6. **Autocompletion Nativo no Terminal (`clonaris completion`):**
   * Suporte nativo a autocompletar para **Bash** e **Zsh** com preenchimento inteligente de comandos, arquivos e ramos.
7. **Zero Dependências Externas & Super Otimizado:**
   * Motor de hashing SHA-256 (RFC 6234) implementado do zero.
   * Binário de apenas **~420 KB** graças a LTO e otimização por tamanho (`opt-level = "z"`).

---

## 🚀 Como Compilar e Instalar

```bash
cd /home/minez400/Projetos/clonaris
cargo build --release

# O executável já é instalado em ~/.local/bin/clonaris:
cp target/release/clonaris ~/.local/bin/clonaris

# Ativar autocompletion no seu shell (adicione ao seu ~/.bashrc ou ~/.zshrc):
eval "$(clonaris completion bash)"
```

---

## 🛠️ Comandos Disponíveis

| Comando | Descrição |
| :--- | :--- |
| `clonaris init` | Inicializa um repositório `.clonaris/` com o ramo padrão `main`. |
| `clonaris status` | Mostra os arquivos novos (`+`), modificados (`~`) ou deletados (`-`). |
| `clonaris diff [arquivo]` | Exibe o diff colorido linha a linha das alterações atuais. |
| `clonaris save -m "mensagem"` | Cria um snapshot atômico direto de todo o estado atual. |
| `clonaris amend -m "mensagem"` | Atualiza o snapshot mais recente preservando o histórico de undo. |
| `clonaris discard <arquivo\|.>` | Descarta alterações locais em um arquivo específico ou em tudo. |
| `clonaris resolve <arquivo>` | Resolve conflitos de merge interativamente (`--current` ou `--target`). |
| `clonaris merge <ramo>` | Mescla outro ramo (3-way merge inteligente e conflitos não-bloqueantes). |
| `clonaris log` | Exibe o histórico de snapshots com árvore ASCII e nós de merge. |
| `clonaris ops` | Exibe a linha do tempo legível de operações (alternativa ao `reflog`). |
| `clonaris undo` | Desfaz a última operação registrada restaurando os arquivos em disco. |
| `clonaris redo` | Refaz a última alteração desfeita, avançando no tempo com segurança. |
| `clonaris branch [nome]` | Lista os ramos disponíveis ou cria um novo ramo no snapshot atual. |
| `clonaris switch <ramo>` | Alterna entre ramos e restaura os arquivos correspondentes em disco. |
| `clonaris tag [nome]` | Lista as tags ou cria uma tag estática no snapshot atual. |
| `clonaris completion [shell]` | Gera script de autocompletar para `bash` ou `zsh`. |
| `clonaris help` | Exibe a tela de ajuda com todos os comandos e parâmetros. |
