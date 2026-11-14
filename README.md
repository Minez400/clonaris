# 🪐 Clonaris VCS

**Clonaris** é um sistema de controle de versão (VCS) moderno, ultrarrápido e sem atritos, construído em **100% Rust Puro** com **ZERO dependências externas** (utilizando exclusivamente a biblioteca padrão `std`).

---

## ⚡ Por que Clonaris é melhor e mais simples que o Git?

1. **Sem Staging Area Obrigatória (`git add`):** 
   No Git, você é obrigado a adicionar arquivos na "staging area" antes de comitar. No **Clonaris**, o comando `save -m "mensagem"` captura diretamente o snapshot das suas alterações reais no diretório.
2. **Viagem no Tempo Bidirecional (`undo` e `redo`):**
   Esqueça comandos assustadores como `git reset --hard` ou garimpar o `git reflog`. O Clonaris mantém um `oplog` e `redolog` sequenciais:
   * Errou? `clonaris undo`.
   * Desfez por engano? `clonaris redo`.
3. **Diff Linha a Linha Colorido Embutido:**
   * Algoritmo de LCS (Longest Common Subsequence) puro em Rust.
   * Exibe adições em verde (`+`) e remoções em vermelho (`-`).
   * Detecta arquivos binários automaticamente para evitar poluição visual.
4. **Zero Dependências Externas:**
   * Motor de hashing SHA-256 (RFC 6234) implementado do zero.
   * Compilação instantânea (menos de 0.4s).
   * Binário de apenas ~600 KB.
5. **Armazenamento Transparente:**
   Objetos imutáveis baseados em conteúdo (*Content-Addressable Store*) gravados em `.clonaris/objects/`.

---

## 🚀 Como Compilar e Instalar

```bash
cd /home/minez400/Projetos/clonaris
cargo build --release

# Opcional: instalar no PATH local (~/.cargo/bin)
cargo install --path .
```

---

## 🛠️ Comandos Disponíveis

| Comando | Descrição |
| :--- | :--- |
| `clonaris init` | Inicializa um repositório `.clonaris/` com o ramo padrão `main`. |
| `clonaris status` | Mostra os arquivos novos (`+`), modificados (`~`) ou deletados (`-`). |
| `clonaris diff [arquivo]` | Exibe o diff colorido linha a linha das alterações atuais. |
| `clonaris save -m "mensagem"` | Cria um snapshot atômico direto de todo o estado atual. |
| `clonaris log` | Exibe o histórico de snapshots com árvore ASCII e metadados. |
| `clonaris undo` | Desfaz a última operação registrada com reversão do estado em disco. |
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
├── oplog         # Log de operações para permitir `clonaris undo`
├── redolog       # Pilha de operações desfeitas para permitir `clonaris redo`
├── objects/      # Armazém de Blobs, Trees e Commits (endereçados por SHA-256)
│   ├── 2f/
│   │   └── 3993...
└── refs/
    ├── heads/    # Ponteiros dos ramos com o hash do último snapshot
    │   └── main
    └── tags/     # Tags estáticas imutáveis (ex: v1.0.0)
```

---

## 🧪 Exemplo de Uso Completo

```bash
# 1. Inicializar
clonaris init

# 2. Criar e salvar
echo -e "linha 1\nlinha 2" > index.txt
clonaris save -m "Versão 1"
clonaris tag v1.0.0

# 3. Editar e inspecionar diff
echo "linha 3" >> index.txt
clonaris diff

# 4. Salvar versão 2
clonaris save -m "Versão 2"

# 5. Viagem no tempo
clonaris undo   # Volta para a Versão 1!
cat index.txt
clonaris redo   # Avança de volta para a Versão 2!
cat index.txt
```
