# 🪐 Clonaris VCS

**Clonaris** é um sistema de controle de versão (VCS) moderno, ultrarrápido e sem atritos, construído em **100% Rust Puro** com **ZERO dependências externas** (utilizando exclusivamente a biblioteca padrão `std`).

---

## ⚡ Por que Clonaris é mais simples que o Git?

1. **Sem Staging Area Obrigatória (`git add`):** 
   No Git, você é obrigado a adicionar arquivos na "staging area" antes de comitar. No **Clonaris**, o comando `save -m "mensagem"` captura diretamente o snapshot das suas alterações reais no diretório.
2. **`undo` Universal e Seguro:**
   Esqueça comandos assustadores como `git reset --hard` ou procurar no `git reflog`. O Clonaris mantém um `oplog` sequencial. Para desfazer qualquer snapshot ou operação errada, basta rodar `clonaris undo`.
3. **Zero Dependências Externas:**
   * Motor de hashing SHA-256 (RFC 6234) implementado do zero em Rust puro.
   * Compilação instantânea (menos de 0.3s).
   * Sem riscos de cadeia de suprimentos ou macros lentas.
4. **Armazenamento Transparente:**
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
| `clonaris save -m "mensagem"` | Cria um snapshot atômico direto de todo o estado atual. |
| `clonaris log` | Exibe o histórico de snapshots com data UTC, autor e árvore. |
| `clonaris undo` | Desfaz a última operação registrada com reversão do estado em disco. |
| `clonaris branch [nome]` | Lista os ramos disponíveis ou cria um novo ramo no snapshot atual. |
| `clonaris switch <ramo>` | Alterna entre ramos e restaura os arquivos correspondentes em disco. |
| `clonaris help` | Exibe a tela de ajuda com todos os comandos e parâmetros. |

---

## 📁 Estrutura Interna (`.clonaris/`)

```text
.clonaris/
├── config        # Configurações do usuário (nome e email)
├── HEAD          # Ponteiro para o ramo atual (ex: ref: refs/heads/main)
├── oplog         # Log de operações para permitir `clonaris undo`
├── objects/      # Armazém de Blobs, Trees e Commits (endereçados por SHA-256)
│   ├── 59/
│   │   └── 8d01...
└── refs/
    └── heads/    # Ponteiros dos ramos com o hash do último snapshot
        └── main
```

---

## 🧪 Exemplo de Fluxo

```bash
# 1. Inicializar
clonaris init

# 2. Criar e inspecionar arquivos
echo "console.log('Olá mundo');" > app.js
clonaris status

# 3. Salvar diretamente
clonaris save -m "Adiciona app.js inicial"

# 4. Modificar e reverter
echo "bug" >> app.js
clonaris save -m "Commit com erro"
clonaris undo # Restaura app.js ao estado anterior imediatamente!
```
