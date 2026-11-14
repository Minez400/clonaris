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

### Core Workflow
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
| `clonaris branch [name]` | List branches or create a new branch pointer (`feature/login` supported). |
| `clonaris switch <branch>` | Switch active branch with data-loss protection (Smart Checkout). |
| `clonaris tag [name]` | List tags or create an immutable static tag (`v1.0.0`). |
| `clonaris log` | Display snapshot history with an ASCII graph tree. |
| `clonaris ops` | Display the chronological transaction log (alternative to `reflog`). |
| `clonaris undo` | Safely revert the latest operation and restore disk files. |
| `clonaris redo` | Reapply the most recently undone operation. |

### 🌐 GitHub & Remote Synchronization
| Command | Description |
| :--- | :--- |
| `clonaris remote [add\|rm\|list]` | Configure and list remote endpoints (`origin`, `upstream`). |
| `clonaris push [-u] [remote] [branch]` | Export snapshots and push to GitHub, GitLab, or remote Git servers. |
| `clonaris pull [remote] [branch]` | Fetch remote updates and merge or fast-forward active branch. |
| `clonaris clone <url> [destination]` | Clone remote repository and bootstrap Clonaris environment instantly. |

### ⚡ Developer Superpowers
| Command | Description |
| :--- | :--- |
| `clonaris stash [push\|pop\|list\|drop]` | Save uncommitted changes to lightweight shelves for fast context switching. |
| `clonaris cherry-pick <commit>` | Apply changes from a specific commit onto the current active branch. |
| `clonaris doctor` | Full integrity audit: recomputes SHA-256 for all CAS objects and validates refs. |
| `clonaris gc` | Generational garbage collection: prunes orphaned objects and reclaims disk space. |
| `clonaris completion [shell]` | Generate shell autocompletion script (`bash` or `zsh`). |
| `clonaris help` | Display comprehensive help and usage information. |

---

## 📁 Repository Internal Layout (`.clonaris/`)

```text
.clonaris/
├── config        # Identity ([user]) and remotes ([remote "origin"])
├── HEAD          # Symbolic reference (e.g., ref: refs/heads/main)
├── oplog         # Append-only transaction log for `clonaris ops` and `undo`
├── redolog       # Undone operations stack for `clonaris redo`
├── objects/      # Content-Addressed Store for Blobs, Trees, and Commits (SHA-256)
│   ├── b0/
│   │   └── 2d20...
├── stashes/      # Saved stash metadata files (`stash_<timestamp>.stash`)
└── refs/
    ├── heads/    # Branch pointers storing snapshot hashes (e.g. main, feature/auth)
    ├── tags/     # Static tag pointers (e.g., v1.0.0)
    └── remotes/  # Remote tracking branches (e.g. origin/main)
```

---

## 🧪 Usage Examples

### 1. Basic Workflow
```bash
# Initialize and commit
clonaris init
echo "fn main() {}" > main.rs
clonaris save -m "Initial commit"
clonaris tag v1.0.0

# Create and switch branches
clonaris branch feature/login
clonaris switch feature/login
echo "fn login() {}" >> main.rs
clonaris save -m "Add login functionality"

# Switch back and merge
clonaris switch main
clonaris merge feature/login
```

### 2. GitHub Synchronization
```bash
# Configure remote
clonaris remote add origin git@github.com:username/my-project.git

# Push current branch to GitHub
clonaris push -u origin main

# Pull remote changes from team members
clonaris pull origin main
```

### 3. Developer Superpowers: Stash & Cherry-Pick
```bash
# Temporarily stash unfinished work
echo "WIP changes" >> main.rs
clonaris stash push "Unfinished experiment"

# Verify workspace is clean
clonaris status

# Pop stash back when ready
clonaris stash pop

# Cherry-pick a specific fix from another branch
clonaris cherry-pick 4a8f9c12

# Run health doctor and garbage collection
clonaris doctor
clonaris gc
```
