# 🪐 Clonaris VCS

**Clonaris** is a modern, ultra-fast, and frictionless version control system (VCS) built in **100% Pure Rust** with **ZERO external dependencies** (using exclusively the Rust Standard Library `std`).

---

## ⚡ Why Clonaris is Simpler and Better than Git

1. **First-Class Non-Blocking Conflicts:**
   * *In Git:* A merge conflict locks your repository in an interim state, halting your workflow until manually resolved or aborted.
   * *In Clonaris:* Branch merges (`clonaris merge <branch>`) record the snapshot with both parent hashes and standard conflict markers. **Your repository is NEVER locked.** You can switch branches, inspect, test, and resolve at your own pace.
2. **Interactive Conflict Resolution (`clonaris resolve <file>`):**
   * No need to manually hunt down and delete `<<<<<<<` and `>>>>>>>` markers. Resolve conflicts interactively or use automated flags (`--current` / `--target`).
3. **Selective File Discard (`clonaris discard <file>`):**
   * Discard uncommitted modifications to a specific file or the entire working tree (`.`) without rolling back snapshots.
4. **Human-Readable Transaction Timeline (`clonaris ops`):**
   * Forget the cryptic `git reflog`. Clonaris maintains a numbered chronological audit log with timestamps, actions, snapshot hashes, and descriptions.
5. **Bidirectional Time Travel (`undo` & `redo`):**
   * Mistake during a commit, merge, or switch? Run `clonaris undo`. Undid something by accident? Run `clonaris redo`.
6. **No Staging Area Friction (Zero Staging):**
   * No mandatory `git add`. The `save -m "message"` command captures working tree changes directly into an atomic snapshot.
7. **Safe, Traceable Amend (`clonaris amend`):**
   * In Git, `commit --amend` destroys the previous commit. In Clonaris, amend is safely recorded in the transaction log and can be reverted with `undo`.
8. **Native Shell Autocompletion:**
   * Built-in completion generator for **Bash** and **Zsh** supporting commands, options, file paths, and branch names.
9. **Zero Dependencies & Aggressive Optimization:**
   * Pure Rust SHA-256 (RFC 6234) implementation.
   * Tiny standalone binary of only **~427 KB** thanks to LTO and code-size optimizations.

---

## 🚀 Installation & Build

```bash
cd /home/minez400/Projetos/clonaris
cargo build --release

# Install binary to ~/.local/bin:
cp target/release/clonaris ~/.local/bin/clonaris

# Activate shell autocompletion (add to ~/.bashrc or ~/.zshrc):
eval "$(clonaris completion bash)"
```

---

## 🛠️ Command Reference

| Command | Description |
| :--- | :--- |
| `clonaris init` | Initialize an empty repository with default branch `main`. |
| `clonaris status` | Show working tree status: added (`+`), modified (`~`), deleted (`-`). |
| `clonaris diff [file]` | Display line-by-line color diffs against the latest snapshot. |
| `clonaris save -m "msg"` | Create an atomic snapshot directly without staging. |
| `clonaris amend -m "msg"` | Safely update the latest snapshot preserving undo history. |
| `clonaris discard <file\|.>` | Discard uncommitted modifications in a specific file or all files. |
| `clonaris resolve <file>` | Resolve merge conflict markers interactively (`--current` or `--target`). |
| `clonaris merge <branch>` | Merge another branch with 3-way reconciliation and non-blocking conflicts. |
| `clonaris log` | Display snapshot history with an ASCII graph tree. |
| `clonaris ops` | Display the chronological transaction log (alternative to `reflog`). |
| `clonaris undo` | Safely revert the latest operation and restore disk files. |
| `clonaris redo` | Reapply the most recently undone operation. |
| `clonaris branch [name]` | List branches or create a new branch pointer (`feature/login` supported). |
| `clonaris switch <branch>` | Switch active branch with data-loss protection (Smart Checkout). |
| `clonaris tag [name]` | List tags or create an immutable static tag (`v1.0.0`). |
| `clonaris completion [shell]` | Generate shell autocompletion script (`bash` or `zsh`). |
| `clonaris help` | Display help and usage information. |

---

## 📁 Repository Internal Layout (`.clonaris/`)

```text
.clonaris/
├── config        # User identity configuration (name and email)
├── HEAD          # Symbolic reference (e.g., ref: refs/heads/main)
├── oplog         # Append-only transaction log for `clonaris ops` and `undo`
├── redolog       # Undone operations stack for `clonaris redo`
├── objects/      # Content-Addressed Store for Blobs, Trees, and Commits (SHA-256)
│   ├── b0/
│   │   └── 2d20...
└── refs/
    ├── heads/    # Branch pointers storing snapshot hashes
    │   ├── main
    │   └── feature/auth
    └── tags/     # Static tag pointers (e.g., v1.0.0)
```

---

## 🧪 Usage Example

```bash
# 1. Initialize
clonaris init

# 2. Create and commit directly
echo "fn main() {}" > main.rs
clonaris status
clonaris save -m "Initial commit"
clonaris tag v1.0.0

# 3. Create a hierarchical feature branch
clonaris branch feature/login
clonaris switch feature/login
echo "fn login() {}" >> main.rs
clonaris save -m "Add login functionality"

# 4. Switch back to main and merge
clonaris switch main
clonaris merge feature/login

# 5. Inspect timeline and undo/redo
clonaris ops
clonaris undo
clonaris redo
```
