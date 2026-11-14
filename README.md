# 🪐 Clonaris VCS

**Clonaris** é um sistema de controle de versão (VCS) moderno, ultrarrápido e sem atritos, construído em **100% Rust Puro** com **ZERO dependências externas** (utilizando exclusivamente a biblioteca padrão `std`).

---

## ⚡ Por que Clonaris resolve as maiores dores dos Devs no Git?

1. **Conflitos de Merge Não-Bloqueantes (*First-Class Conflicts*):**
   * *No Git:* O repositório entra em pânico, trava em modo intermediário e impede você de trocar de branch ou executar outros comandos até resolver tudo ou dar abort.
   * *No Clonaris:* O comando `clonaris merge <ramo>` grava o snapshot com ambos os pais e com os marcadores de conflito nos arquivos. **Seu repositório NUNCA fica travado.** Você resolve quando quiser com calma.
2. **Linha do Tempo Humana (`clonaris ops`):**
   * Esqueça o pesadelo ininteligível do `git reflog`. O Clonaris possui uma esteira numerada com data, hora, ação e descrição legível de cada comando executado.
3. **Viagem no Tempo Totalmente Segura (`undo` e `redo`):**
   * Qualquer ação (`save`, `amend`, `merge`, `switch`) pode ser desfeita e refeita instantaneamente sem risco de perda de código.
4. **Sem Staging Area Obrigatória (`git add`):**
   * No **Clonaris**, o comando `save -m "mensagem"` captura diretamente o snapshot das suas alterações reais no diretório.
5. **Amend Seguro e Reversível (`clonaris amend`):**
   * No Git, `commit --amend` destrói o commit anterior. No Clonaris, o amend é registrado no OpLog e pode ser revertido com `undo`.
6. **Diff Linha a Linha Colorido Embutido:**
   * Algoritmo de LCS (Longest Common Subsequence) puro em Rust com detecção de binários.
7. **Zero Dependências Externas:**
   * Motor de hashing SHA-256 (RFC 6234) implementado do zero.
   * Compila em menos de 0.5s e gera um executável de apenas ~600 KB.

---

## 🚀 Como Compilar e Instalar

```bash
cd /home/minez400/Projetos/clonaris
cargo build --release

# O executável já é instalado em:
cp target/release/clonaris ~/.local/bin/clonaris
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
| `clonaris merge <ramo>` | Mescla outro ramo (3-way merge inteligente e conflitos não-bloqueantes). |
| `clonaris log` | Exibe o histórico de snapshots com árvore ASCII e nós de merge. |
| `clonaris ops` | Exibe a linha do tempo legível de operações (alternativa ao `reflog`). |
| `clonaris undo` | Desfaz a última operação registrada restaurando os arquivos em disco. |
| `clonaris redo` | Refaz a última alteração desfeita, avançando no tempo com segurança. |
| `clonaris branch [nome]` | Lista os ramos disponíveis ou cria um novo ramo no snapshot atual. |
| `clonaris switch <ramo>` | Alterna entre ramos e restaura os arquivos correspondentes em disco. |
| `clonaris tag [nome]` | Lista as tags ou cria uma tag estática no snapshot atual. |
| `clonaris help` | Exibe a tela de ajuda com todos os comandos e parâmetros. |

---

## 📁 Estrutura Interna (`.clonaris/`)

```text
.clonaris/
├── config        # Configurações do usuário (nome e email)
├── HEAD          # Ponteiro para o ramo atual (ex: ref: refs/heads/main)
├── oplog         # Linha do tempo de operações legível para `clonaris ops` e `undo`
├── redolog       # Pilha de operações desfeitas para permitir `clonaris redo`
├── objects/      # Armazém de Blobs, Trees e Commits (endereçados por SHA-256)
│   ├── b0/
│   │   └── 2d20...
└── refs/
    ├── heads/    # Ponteiros dos ramos com o hash do último snapshot
    │   ├── main
    │   └── feature
    └── tags/     # Tags estáticas imutáveis (ex: v1.0.0)
```

---

## 🧪 Exemplo de Uso Avançado

```bash
# 1. Inicializar e salvar
clonaris init
echo "linha base" > main.txt
clonaris save -m "Base inicial"

# 2. Criar e trabalhar em um ramo
clonaris branch feature
clonaris switch feature
echo "feature code" >> main.txt
clonaris save -m "Trabalho na feature"

# 3. Alteração concorrente no ramo principal
clonaris switch main
echo "main code" >> main.txt
clonaris save -m "Trabalho no main"

# 4. Mesclar (Conflito Não-Bloqueante!)
clonaris merge feature
# O repositório NÃO trava! O snapshot é gravado com marcadores e você pode
# inspecionar com 'clonaris diff' e resolver quando quiser.

# 5. Inspecionar a linha do tempo
clonaris ops

# 6. Desfazer ou refazer qualquer coisa
clonaris undo
clonaris redo
```
