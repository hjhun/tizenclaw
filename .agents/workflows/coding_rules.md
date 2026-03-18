---
description: TizenClaw Coding Rules and Guidelines
---

# TizenClaw Agent Support Rules

When implementing TizenClaw in this repository, the Agent (AI) must **strictly** prioritize and adhere to the following coding styles and rules, which are derived directly from the project author's style guidelines.

## 1. C++ Coding Style
- **C++ Standard**: Use **C++20** (`-std=c++20`).
- **Style Guide**: Strictly follow the [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html).
- **Line Wrap & Formatting**:
  - **Column Limit**: Ensure all text in source code, comments, and header files is appropriately wrapped **not to exceed 80 characters (Column limit: 80)**.
  - **Indentation**: Use **2 spaces** (Space 2) strictly. Do not use tabs, and NEVER use 4 spaces.
  - **Brace Placement**: Use K&R/Stroustrup style. The opening brace `{` must be on the same line as the statement (e.g., `if (...) {`, `void Func() {`).
  - **Single-statement if/else**: If the block contains only a single statement, **omit the braces**. (e.g., `if (!exit_timer_) return;`)
  - **Access Modifiers**: `public:`, `private:`, `protected:` must be indented by **1 space** (` public:`).
  - **Namespaces**: Code inside a `namespace` block should **not** be indented.
  - **Pointers & References**: Attach `*` and `&` to the type, not the variable name. (e.g., `char** argv_`, `const std::string& package`)
- **Naming Conventions**:
  - **Class/Struct/Namespace**: `PascalCase` (e.g., `Watcherd`, `AgentCore`).
  - **Functions/Methods**: `PascalCase` (e.g., `HandlePkgmgrEventStart`, `SetExitTimer`).
  - **Local Variables/Parameters**: `snake_case` (e.g., `exit_timer`, `loader`).
  - **Member Variables**: `snake_case` with a single trailing underscore `_` (e.g., `argc_`, `exit_timer_`). **Never use the `m_` prefix.**
  - **Constants/Enums**: Prefix with a lowercase `k` followed by `PascalCase` (e.g., `kRegularUidMin`, `kMaxHistorySize`).
- **Includes & Headers**:
  - Use `#ifndef FILENAME_HH_` style header guards, matching the `.hh` extension.
  - Group includes logically: C system libs ➡️ C++ standard libs ➡️ Local project headers. Add a blank line between each group.
- **Modern C++ Features**:
  - Use `std::make_unique` and `std::shared_ptr` for resource management. Avoid raw `new`/`delete` where possible.
  - Use anonymous namespaces `namespace { ... }` in `.cc` files for internal linkage instead of `static`.
  - `[[nodiscard]]`: Apply to bool/state returning functions.
  - `std::filesystem`: Use instead of POSIX `opendir/readdir/stat`.
  - `map::contains()`: Use instead of `find() != end()`.
  - `std::ranges`: Prioritize range-based algorithms.
  - `using enum`: Apply for repeated enumeration use within scope.
- **Variable Shadowing (`-Wshadow`)**:
  - **NEVER** declare a variable with the same name as an existing variable in an outer scope. The project compiles with `-Wshadow`, and some build environments treat this as `-Werror=shadow`, causing build failures.
  - This is especially critical inside **lambdas** and **nested blocks** (e.g., `if`/`for` bodies). When a lambda captures or receives a variable from its enclosing scope, the inner variable must use a distinct name.
  - **Bad**: Reusing `ret`, `ctx`, `manifest` inside a lambda/inner block when the same name exists in the outer function scope.
  - **Good**: Use descriptive, disambiguated names like `metadata_ret`, `iter_ctx`, `manifest_path` for inner variables.
- **Unused Code (`-Wunused`)**:
  - The project compiles with `-Wunused`. **NEVER** leave unused functions, variables, or includes in the code.
  - If a helper function is no longer called, **remove it immediately** rather than leaving it for later.
  - If a function parameter is intentionally unused (e.g., callback signatures), cast to `void` or use `[[maybe_unused]]`: `[[maybe_unused]] int fd`.
  - **Note**: `-Wno-unused-parameter` and `-Wno-unused-result` are currently suppressed project-wide, so unused *parameters* and *return values* won't trigger warnings. But unused *functions* and *local variables* will.

## 2. Clean Code & Effective C++ Principles
- **Effective C++ (Scott Meyers)**:
  - **Item 3/16**: Proactively use `const`. All getter methods and any function not modifying class state MUST be marked `const`. Variables whose values never change should be `const`.
  - **Item 20**: Prefer pass-by-reference-to-const for complex objects (`const std::string&`, `const std::vector&`) over pass-by-value.
  - **Item 22**: Declare data members `private`. Never expose class variables publicly without a clear reason.
  - **Modern Item 15**: Use `constexpr` proactively for simple constants to allow compile-time evaluation.
- **Clean Code (Robert C. Martin)**:
  - **Meaningful Names**: Don't use magic numbers. Name your variables and functions clearly to express intent.
  - **Early Returns (Guard Clauses)**: Never deeply nest your conditional logic (3+ levels deep). Instead, invert the `if` and return/continue/break early to reduce indentations.
  - **Do One Thing**: Break huge functions into smaller, private helper methods. A function should ideally fit on a single screen without needing to scroll endlessly.

## 3. CMake and Build Support
- Written targeting the Tizen GBS (Gerrit Build System) environment, `gbs build` must always succeed via CMake.
- When adding new C++ source files, you must update the `SOURCES` list in `CMakeLists.txt`.
- **Warning Flags**: The project compiles with `-Wall -Wextra -Wshadow -Wunused -fPIC`. All code must compile **warning-free** under these flags.

## 4. Tizen-Specific Rules
- Features requiring privileges (Network, LXC execution, AppManager, etc.) must be explicitly stated in the `<privileges>` block of `tizen-manifest.xml`.
- Make full use of the dlog interface (`dlog_print`) to leave comprehensive system logs, and prioritize error handling via return codes or boolean returns over C++ exceptions whenever possible.
