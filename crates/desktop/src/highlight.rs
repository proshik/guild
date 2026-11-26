//! Which language a file is, and whether it is text at all.
//!
//! The highlighting itself is the editor's, through tree-sitter's queries;
//! what is ours is the table that lands a grammar's captures on the seven
//! roles section 2.4 of the design system colours, and the two questions
//! a file tab asks before it opens anything: what language, and is this
//! text.

use std::path::Path;

/// A language: one row of a table, in the shape of an editor extension's
/// `config.toml`.
///
/// The shape is not a taste. A row that looks like this can one day
/// arrive from a file or from a plugin, and a plugin cannot add an arm to
/// a `match` — which is what the six languages of M8.1 were. `server` and
/// `debug` were empty everywhere when they arrived, and were there for the
/// same reason: the row is where a language server and a debug adapter are
/// named, and leaving the fields out would have meant rewriting every row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// What the status bar says, and the language's own name.
    pub name: &'static str,
    /// The grammar that parses it — **not always its own**. `.ini` is
    /// read by TOML's parser and `Cargo.lock` by TOML's too; the two
    /// being separate fields is the only answer to `lock`, which is one
    /// suffix over four formats.
    pub grammar: &'static str,
    /// Suffixes, lowercase and without their dot. The longest match wins.
    pub suffixes: &'static [&'static str],
    /// Whole names, lowercase. Beat every suffix.
    pub filenames: &'static [&'static str],
    /// Interpreters this language answers to in a `#!` line, read only
    /// when the name said nothing.
    pub shebangs: &'static [&'static str],
    /// The language server that reads it: **the command and its
    /// arguments**, empty when the language has none. M8.5.
    ///
    /// A slice rather than a name because a server is often a command
    /// with a flag — `typescript-language-server --stdio` — and a
    /// slice rather than a line because nothing here runs a shell: a
    /// server started through one could not be stopped by its group.
    pub server: &'static [&'static str],
    /// Files one of which the project's root must hold for the server to
    /// be started, empty when it starts anywhere.
    ///
    /// **A server that knows only a described project is worse than none
    /// outside one.** sourcekit-lsp in an Xcode project with no
    /// `buildServer.json` compiles each file alone and publishes forty
    /// errors a file about the names its neighbours declare; no server
    /// said nothing. Not started, the language says why in the status
    /// bar, naming these files.
    pub server_needs: &'static [&'static str],
    /// The debug adapter, when there is one. Empty until the debugger.
    pub debug: Option<&'static str>,
    /// The file that marks the root of a **dependency** written in this
    /// language — `Cargo.toml`, `go.mod`, `package.json`. Empty where
    /// there is nothing to find. M8.8.3.
    ///
    /// **This is the whole of External Libraries.** A full IDE answers the
    /// question with a build system and a library-roots provider per
    /// language; that model is one we are not growing. What replaces it is
    /// one rule — *the nearest ancestor of the landed file that holds the
    /// language's manifest* — and the rule is a column here for the same
    /// reason `server` is: a row can one day arrive from outside, and a
    /// function per language cannot.
    ///
    /// A slice, because a language may mark a root in more than one way,
    /// and because that is the shape `suffixes` and `filenames` already
    /// have.
    pub manifest: &'static [&'static str],
    /// What Tab writes in a file of this language. M5.11's decision 9.
    ///
    /// **The language's own tool decides, and nothing else is read.**
    /// `.editorconfig` and the indent of the file's own lines are the
    /// right next step, and both belong with the Editor section of
    /// Settings, where a width is a setting.
    pub indent: Indent,
}

/// What Tab writes: a number of spaces, or a hard tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    /// This many spaces.
    Spaces(u8),
    /// One `\t`.
    Tabs,
}

/// The indent of a row that says nothing, and of a file with no
/// language: four spaces, the common default.
pub const PLAIN_INDENT: Indent = Indent::Spaces(4);

/// The grammar a file with no language is given. It must be registered:
/// the component falls back to it, and *unwraps* the fallback.
pub const PLAIN: &str = "text";

/// A row with nothing but a name and a grammar, so the table below reads
/// as a table rather than as a column of `None`s.
const fn row(
    name: &'static str,
    grammar: &'static str,
    suffixes: &'static [&'static str],
    filenames: &'static [&'static str],
    shebangs: &'static [&'static str],
) -> Language {
    Language {
        name,
        grammar,
        suffixes,
        filenames,
        shebangs,
        server: &[],
        server_needs: &[],
        debug: None,
        manifest: &[],
        indent: PLAIN_INDENT,
    }
}

impl Language {
    /// The same row with a server named. Separate from [`row`] so that
    /// the table stays a table: most rows say nothing about servers, and
    /// the ones that do say it where they are read.
    const fn serves(mut self, server: &'static [&'static str]) -> Self {
        self.server = server;
        self
    }

    /// The same row with the files its server needs in the project's
    /// root, one of them.
    const fn needs(mut self, files: &'static [&'static str]) -> Self {
        self.server_needs = files;
        self
    }

    /// The same row with the file that marks a dependency's root. Kept
    /// off [`row`] for the reason `serves` is: most rows have nothing to
    /// say about it.
    const fn packaged_by(mut self, manifest: &'static [&'static str]) -> Self {
        self.manifest = manifest;
        self
    }

    /// The same row with an indent other than [`PLAIN_INDENT`].
    const fn indents(mut self, indent: Indent) -> Self {
        self.indent = indent;
        self
    }
}

/// A recipe line of a Makefile **must** begin with a tab — `make` says
/// *missing separator* otherwise — and `gofmt` writes nothing else.
const TAB: Indent = Indent::Tabs;
/// Prettier's width, which every file of these languages passes through
/// in a project that has a formatter at all.
const PRETTIER: Indent = Indent::Spaces(2);
/// RuboCop's.
const RUBOCOP: Indent = Indent::Spaces(2);
/// `mix format`'s.
const MIX_FORMAT: Indent = Indent::Spaces(2);
/// nixfmt's.
const NIXFMT: Indent = Indent::Spaces(2);
/// `terraform fmt`'s.
const TERRAFORM_FMT: Indent = Indent::Spaces(2);
/// Google's shell style guide.
const GOOGLE_SHELL: Indent = Indent::Spaces(2);
/// `buf format`'s.
const BUF_FORMAT: Indent = Indent::Spaces(2);

impl Language {
    /// The table. Nineteen kinds of file were asked for; the rows that
    /// ride another language's grammar — `ini`, `jsonc` — are the ones
    /// that cost nothing to add.
    pub const ALL: &'static [Language] = &[
        // **A server is named only once it has been asked**, decision 2
        // of M8.5: a row for a server nobody here has run is a row nobody
        // has checked. rust-analyzer was the first; gopls, ruby-lsp,
        // jdtls, kotlin-lsp and expert followed on 2026-09-17, and
        // sourcekit-lsp on 2026-09-28, each asked for a definition in a
        // two-file project of its language. The rest are a line each when
        // somebody wants them.
        row("rust", "rust", &["rs"], &[], &[])
            .serves(&["rust-analyzer"])
            .packaged_by(&["Cargo.toml"]),
        row("go", "go", &["go"], &[], &[])
            .serves(&["gopls"])
            .packaged_by(&["go.mod"])
            .indents(TAB),
        row(
            "ruby",
            "ruby",
            &["rb", "rake", "gemspec"],
            &["gemfile", "rakefile"],
            &["ruby"],
        )
        // Shopify's. In a project with a `Gemfile` it wants the
        // `Gemfile.lock` beside it, and exits saying so where there is none.
        .serves(&["ruby-lsp"])
        .packaged_by(&["Gemfile"])
        .indents(RUBOCOP),
        row(
            "elixir",
            "elixir",
            &["ex", "exs"],
            &["mix.lock"],
            &["elixir"],
        )
        // The official one, which ElixirLS, Lexical and Next LS merged
        // into. Without `--stdio` it asks for a port.
        .serves(&["expert", "--stdio"])
        .packaged_by(&["mix.exs"])
        .indents(MIX_FORMAT),
        // The Java language server. Homebrew's `jdtls` keeps its workspace
        // under the user's cache, named by the directory it is started in, so
        // two projects do not share an index.
        row("java", "java", &["java"], &[], &[]).serves(&["jdtls"]),
        // The `kotlin-lsp` server. Without `--stdio` it listens on a socket;
        // it imports a project through Gradle or Maven, and a directory with
        // neither answers every definition with nothing. Homebrew's cask is
        // quarantined, and until the quarantine is lifted macOS refuses its
        // file watcher and the server exits at once.
        row("kotlin", "kotlin", &["kt", "kts"], &[], &[]).serves(&["kotlin-lsp", "--stdio"]),
        row(
            "javascript",
            "javascript",
            &["js", "mjs", "cjs", "jsx"],
            &[],
            &["node"],
        )
        .packaged_by(&["package.json"])
        .indents(PRETTIER),
        row("typescript", "typescript", &["ts", "mts", "cts"], &[], &[])
            .packaged_by(&["package.json"])
            .indents(PRETTIER),
        row("tsx", "tsx", &["tsx"], &[], &[])
            .packaged_by(&["package.json"])
            .indents(PRETTIER),
        row(
            "json",
            "json",
            &["json"],
            &[
                "package-lock.json",
                "flake.lock",
                ".babelrc",
                ".eslintrc",
                ".prettierrc",
            ],
            &[],
        )
        .indents(PRETTIER),
        row(
            "jsonc",
            "json",
            &["jsonc", "json5"],
            &["tsconfig.json"],
            &[],
        )
        .indents(PRETTIER),
        row("yaml", "yaml", &["yml", "yaml"], &[], &[]).indents(PRETTIER),
        row(
            "make",
            "make",
            &["mk"],
            &["makefile", "gnumakefile"],
            &["make"],
        )
        .indents(TAB),
        row("just", "just", &["just"], &["justfile", ".justfile"], &[]),
        row(
            "dockerfile",
            "dockerfile",
            &["dockerfile"],
            &["dockerfile", "containerfile"],
            &[],
        ),
        row(
            "toml",
            "toml",
            &["toml"],
            &["cargo.lock", "poetry.lock", "pipfile"],
            &[],
        ),
        row(
            "ini",
            "toml",
            &["ini", "conf", "cfg"],
            &[".editorconfig", ".gitconfig"],
            &[],
        ),
        row("properties", "properties", &["properties"], &[], &[]),
        row("markdown", "markdown", &["md", "markdown", "mdx"], &[], &[]).indents(PRETTIER),
        row(
            "bash",
            "bash",
            &["sh", "bash", "zsh"],
            &[
                ".zshrc",
                ".bashrc",
                ".zprofile",
                ".bash_profile",
                ".profile",
            ],
            &["sh", "bash", "zsh", "dash"],
        )
        .indents(GOOGLE_SHELL),
        row("proto", "proto", &["proto"], &[], &[]).indents(BUF_FORMAT),
        // M8.8.6. The web four first: html is the one whose colour is
        // almost entirely somebody else's, through the injections its
        // own thirteen-line query hands out.
        row("html", "html", &["html", "htm", "xhtml"], &[], &[]).indents(PRETTIER),
        row("css", "css", &["css"], &[], &[]).indents(PRETTIER),
        row("scss", "scss", &["scss"], &[], &[]).indents(PRETTIER),
        row("svelte", "svelte", &["svelte"], &[], &[]).indents(PRETTIER),
        // SVG source uses the same XML editor when its preview is switched to source.
        row(
            "xml",
            "xml",
            &["xml", "xsd", "xsl", "xslt", "svg"],
            &[],
            &[],
        ),
        row("sql", "sql", &["sql"], &[], &[]),
        row(
            "python",
            "python",
            &["py", "pyi", "pyw"],
            &[],
            &["python", "python3"],
        ),
        // `.h` is C's here rather than C++'s, which is the choice every
        // editor has to make and nobody makes correctly: a header names
        // no language, and C's grammar reads the C++ ones that matter
        // without the reverse being true.
        row("c", "c", &["c", "h"], &[], &[]),
        row(
            "cpp",
            "cpp",
            &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
            &[],
            &[],
        ),
        row("csharp", "csharp", &["cs"], &[], &[]),
        row("php", "php", &["php", "phtml"], &[], &["php"]),
        // Apple's, and on the PATH wherever Xcode or its command line
        // tools are: `/usr/bin/sourcekit-lsp` is xcrun's shim. It knows a
        // SwiftPM package, and an Xcode project through the
        // `buildServer.json` that `xcode-build-server config` writes;
        // anywhere else it compiles each file alone, answers no
        // definition and reports every neighbour's name as missing — so
        // it is started only beside one of the two. It indexes first,
        // and says so as `Indexing`.
        row("swift", "swift", &["swift", "swiftinterface"], &[], &[])
            .serves(&["sourcekit-lsp"])
            .needs(&["Package.swift", "buildServer.json"])
            .packaged_by(&["Package.swift"]),
        row("lua", "lua", &["lua"], &[], &["lua"]),
        row("nix", "nix", &["nix"], &[], &[]).indents(NIXFMT),
        row("graphql", "graphql", &["graphql", "gql"], &[], &[]).indents(PRETTIER),
        // Terraform's files are HCL and so is Packer's; `hcl` itself is
        // the format's own suffix.
        row("hcl", "hcl", &["hcl", "tf", "tfvars"], &[], &[]).indents(TERRAFORM_FMT),
    ];

    /// Whether the server may be started in a project rooted at `root`:
    /// it needs nothing, or the root itself holds one of the files it
    /// needs. The root's own and not a nested package's, because the
    /// root is where the server is started and what it reads.
    pub fn server_starts_in(&self, root: &Path) -> bool {
        self.server_needs.is_empty()
            || self
                .server_needs
                .iter()
                .any(|file| root.join(file).is_file())
    }

    /// The language of a file, from its name alone: the whole name first
    /// — `mix.lock` is Elixir and `package-lock.json` is JSON — then the
    /// suffixes from the longest, which is the rule `icons::for_file` has
    /// followed since M8.1. The loop starts at the second component so a
    /// whole name never reaches the suffix table twice.
    ///
    /// `None` is a real answer: a `.txt` is text, and text has no
    /// grammar.
    pub fn of(path: &Path) -> Option<&'static Language> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Some(found) = Self::ALL
            .iter()
            .find(|language| language.filenames.contains(&name.as_str()))
        {
            return Some(found);
        }
        let parts: Vec<&str> = name.split('.').collect();
        (1..parts.len()).find_map(|i| {
            let suffix = parts[i..].join(".");
            Self::ALL
                .iter()
                .find(|language| language.suffixes.contains(&suffix.as_str()))
        })
    }

    /// The language of a file, its first line consulted **only when the
    /// name said nothing at all**.
    ///
    /// M8.1 refused to look at content because it "would be a guess that
    /// changes as the file is typed". A first line does not change while
    /// the three hundredth is being written, and a script with no
    /// extension has nothing else to offer.
    pub fn of_file(path: &Path, first_line: &str) -> Option<&'static Language> {
        Self::of(path).or_else(|| Self::of_shebang(first_line))
    }

    /// The language a `#!` line names, by the interpreter it runs.
    pub fn of_shebang(first_line: &str) -> Option<&'static Language> {
        let interpreter = Self::interpreter(first_line)?;
        Self::ALL
            .iter()
            .find(|language| language.shebangs.contains(&interpreter))
    }

    /// The program a `#!` line runs, by name: the first word's last
    /// component, and the word after it when that is `env` — which is
    /// how `#!/usr/bin/env -S ruby -w` still says `ruby`.
    fn interpreter(first_line: &str) -> Option<&str> {
        let mut words = first_line.strip_prefix("#!")?.split_whitespace();
        let first = words.next()?.rsplit('/').next()?;
        if first != "env" {
            return Some(first);
        }
        words.find(|word| !word.starts_with('-'))
    }
}

/// The root of the dependency a file belongs to, if it belongs to one.
///
/// **The nearest ancestor holding the language's manifest** — M8.8.3,
/// request 10, and the whole of what stands for External Libraries.
/// It is one rule for every language because `manifest` is a column of
/// the table rather than a function per language, and the two shapes it
/// has to answer were measured rather than guessed:
///
/// - `~/.cargo/registry/src/index.crates.io-<hash>/serde-1.0.228/` with
///   its `Cargo.toml` — the directory a person calls *serde 1.0.228*;
/// - `~/go/pkg/mod/gopkg.in/yaml.v3@v3.0.1/` with its `go.mod`, where
///   the version is in the module directory's own name.
///
/// `None` where there is no manifest above the file — a loose script, a
/// language whose dependencies are jars — and `None` is the answer, not
/// a root invented from the parent directory. The caller says so out
/// loud rather than opening a tab under a root that means nothing.
///
/// `file` is an absolute path: this walks upwards, and a relative path
/// would stop at whatever the process's working directory happens to be.
pub fn dependency_root(file: &Path, language: &Language) -> Option<std::path::PathBuf> {
    if language.manifest.is_empty() {
        return None;
    }
    file.ancestors()
        .skip(1)
        .find(|dir| {
            language
                .manifest
                .iter()
                .any(|manifest| dir.join(manifest).is_file())
        })
        .map(Path::to_path_buf)
}

/// The seven roles section 2.4 colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `keyword`, and everything under it.
    Keyword,
    /// `type`, `constructor`, `enum`, `variant`.
    Type,
    /// `string`, and its escapes.
    String,
    /// `number`, `boolean`, `constant`.
    Number,
    /// `comment`.
    Comment,
    /// `comment_doc`: a documentation comment, with a colour of its own
    /// and leant — M8.10.
    Doc,
    /// `attribute`, `preproc`, and a macro invocation.
    Macro,
    /// `function`, and a Markdown title.
    Function,
    /// `constant`. It rode with `number` until M8.10; now it is apart,
    /// in italic.
    Constant,
    /// `tag`: a markup element's name, coloured as neither a type nor
    /// plain text. M8.10.
    Tag,
    /// A **reference** to a type, for a grammar whose row asks for one
    /// where the table says `type` — M8.10's layer, and Go's teal.
    TypeRef,
    /// A package or module's name — Go's olive.
    Namespace,
    /// A built-in of the language — Go's second orange.
    Builtin,
}

impl Role {
    /// Every role, in the order section 2.4 lists them.
    pub const ALL: &'static [Role] = &[
        Role::Keyword,
        Role::Type,
        Role::String,
        Role::Number,
        Role::Comment,
        Role::Doc,
        Role::Constant,
        Role::Tag,
        Role::TypeRef,
        Role::Namespace,
        Role::Builtin,
        Role::Macro,
        Role::Function,
    ];

    /// The role of a capture, or `None` where the table paints it as
    /// ordinary text rather than with a colour of its own.
    pub fn of_capture(capture: &str) -> Option<Self> {
        match paint_of_capture(capture)? {
            Paint::Role(role) => Some(role),
            _ => None,
        }
    }
}

/// What a capture is painted with: one of the seven colours, or a rank of
/// ordinary text — section 2.4 names those too, and a table that had only
/// the seven would leave `punctuation` and `variable` to be decided
/// somewhere else, which is how two tables start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// One of the seven.
    Role(Role),
    /// `text.primary`: no colour of its own.
    Plain,
    /// `text.secondary`.
    Quiet,
    /// `text.primary`, italic.
    Emphasis,
    /// `text.primary`, bold.
    Strong,
}

/// **The whole of what the window knows about syntax colour**, and the
/// only table that knows it. The component's theme is built from this and
/// our own spans read it, so a capture painted in one and forgotten in
/// the other is a state that cannot be reached — which it could before,
/// and had been for six names.
///
/// Order matters only in that the test compares it with the theme's keys;
/// lookup falls back along the dots.
pub const CAPTURES: &[(&str, Paint)] = &[
    ("keyword", Paint::Role(Role::Keyword)),
    ("type", Paint::Role(Role::Type)),
    ("constructor", Paint::Role(Role::Type)),
    ("enum", Paint::Role(Role::Type)),
    ("variant", Paint::Role(Role::Type)),
    ("string", Paint::Role(Role::String)),
    // A string escape is the keyword's colour, not the string's — M8.10.
    ("string.escape", Paint::Role(Role::Keyword)),
    ("string.special", Paint::Role(Role::String)),
    ("number", Paint::Role(Role::Number)),
    ("boolean", Paint::Role(Role::Number)),
    ("constant", Paint::Role(Role::Constant)),
    ("comment", Paint::Role(Role::Comment)),
    ("comment_doc", Paint::Role(Role::Doc)),
    ("attribute", Paint::Role(Role::Macro)),
    ("preproc", Paint::Role(Role::Macro)),
    ("function", Paint::Role(Role::Function)),
    ("label", Paint::Role(Role::Function)),
    ("title", Paint::Role(Role::Function)),
    ("tag", Paint::Role(Role::Tag)),
    ("property", Paint::Plain),
    ("variable", Paint::Plain),
    ("link_uri", Paint::Role(Role::TypeRef)),
    ("punctuation", Paint::Quiet),
    ("operator", Paint::Quiet),
    // **The three slots a grammar's own paints land on** — M8.10's layer.
    // The component has forty names and these three are used by nothing
    // else; a per-grammar colour has to land on a name it can hold, or
    // the tab and the diff would disagree. Nothing emits them: they are
    // written by `query_for`, which is why both engines still read one
    // query.
    ("predictive", Paint::Role(Role::TypeRef)),
    ("hint", Paint::Role(Role::Namespace)),
    ("primary", Paint::Role(Role::Builtin)),
    ("emphasis", Paint::Emphasis),
    ("emphasis.strong", Paint::Strong),
];

/// Captures the grammars we register do name and the window deliberately
/// leaves grey. The coverage guard walks every query against
/// [`CAPTURES`] and this list; a name in neither is a colour nobody
/// decided about, which is the thing the guard exists to catch.
pub const UNPAINTED: &[&str] = &[
    // A region handed to another grammar. We do not paint the wrapper.
    "embedded",
    // The vendored queries' marker for "spell-check this", not a colour.
    "spell",
    // A parse error. There is no colour for one, and a red file is worse
    // than a plain one.
    "error",
    // `@text.danger` and friends in make's query: a diagnostic rank we do
    // not have.
    "text",
    // A dummy capture make's query pairs with the real one on the same
    // node so that a `#match?` predicate can name it. The same idea with
    // a leading underscore — elixir's `@__name__`, kotlin's `@_class` —
    // is caught by the rule in [`unpainted`] rather than listed.
    "clean",
    // XML's `(CharData) @markup` is the text between the tags — a
    // document's content, which every other language draws plain — and
    // `markup.heading`, `markup.raw` and `markup.link` under it are the
    // `<![CDATA[` markers, what they enclose, and a URI. There is no
    // colour for XML prose in section 2.4; Markdown text.* maps separately.
    "markup",
    // Explicitly *no* highlight, which some grammars say out loud.
    "none",
    // Local-scope bookkeeping that rides in a highlights query.
    "local",
    "definition",
    "reference",
    "scope",
];

/// Captures a grammar names that the component has **no slot for at all**,
/// and the name each is given instead.
///
/// The component's forty capture names are a ceiling: it has no `escape`,
/// no `module`, no `conditional`, and a name without a dot gets no
/// fallback either — so no theme of ours could ever colour them. But the
/// component takes the *query text* from us, so the cure is a rename as
/// the query is registered. Both engines then read one rewritten query,
/// which makes them agree by construction rather than by care.
///
/// **A row here is matched as a prefix of the dots**, not as a whole
/// name: `@module.builtin` in php's query and `@character.special` in
/// swift's are the same defect one level down — the root is renamed, the
/// child is not, and the child then falls back through a root that only
/// [`RENAMED`] has ever heard of. `module.builtin` becomes `type.builtin`
/// (one rename, not a chain: a type to both engines)
/// and `character.special` becomes `string.special` without a row of
/// their own.
pub const RENAMED: &[(&str, &str)] = &[
    // Markdown's upstream query uses `text.*` names; map the specific
    // styles without treating diagnostic text.* captures as syntax.
    ("text.title", "title"),
    ("text.literal", "string"),
    ("text.uri", "link_uri"),
    ("text.reference", "label"),
    ("text.emphasis", "emphasis"),
    ("text.strong", "emphasis.strong"),
    // `(escape_sequence) @escape` in rust, go and json: every escape in
    // every string had been grey since M8.1.
    ("escape", "string.escape"),
    // `(alias) @module` in elixir is `MyApp.Repo`; proto says it too, and
    // php says `@module.builtin` for the ones it ships with.
    ("module", "type"),
    ("namespace", "type"),
    // kotlin's spelling of what section 2.4 calls `variable.parameter`.
    ("parameter", "variable.parameter"),
    // A character literal is a string of one, and section 2.4 has no
    // rank of its own for it.
    ("character", "string"),
    ("float", "number"),
    // make's query uses an older capture vocabulary, and these four are
    // its spellings of a keyword.
    ("conditional", "keyword"),
    ("repeat", "keyword"),
    ("include", "keyword"),
    ("exception", "keyword"),
    // The queries' word for a record's member — sql's and lua's `a.b`. The
    // component calls the same thing a property, and section 2.4 draws a
    // property plain.
    ("field", "property"),
    // Lua's `obj:method()`, which is the construct a Lua file is mostly
    // made of; `@method.call` rides the prefix rule into
    // `function.method.call`, which both engines resolve to `function`.
    ("method", "function.method"),
    // sql's `@storageclass` is `primary key`, `unique`, `default` — a
    // keyword by any other name.
    ("storageclass", "keyword"),
    // c's spelling of the comma and the semicolon.
    ("delimiter", "punctuation.delimiter"),
    // **A built-in type is a keyword, since M8.10** — Go's `int64`, Rust's
    // `u32`. It was a row of `CAPTURES`, and the component has no
    // `type.builtin` slot: it fell back to `type`, so a tab drew `u32` as a
    // type while a diff drew it as a keyword.
    ("type.builtin", "keyword"),
    // c's `#define NAME(…)`: a macro, which the component calls `preproc`.
    // As a row of `CAPTURES` it was the same defect — a macro in a diff, a
    // function in a tab.
    ("function.special", "preproc"),
];

/// Whether a capture is deliberately left grey: one of [`UNPAINTED`], or
/// a helper a predicate names.
///
/// **A leading underscore is the convention for a helper** — elixir's
/// `@__name__`, kotlin's `@_class` and `@_function`. They exist so a
/// `#match?` can refer to the node; painting one would paint whatever the
/// predicate was testing, which is not a colour anybody chose.
pub fn unpainted(capture: &str) -> bool {
    let root = capture.split('.').next().unwrap_or(capture);
    root.starts_with('_') || UNPAINTED.contains(&root)
}

/// What a **grammar** paints a capture with, when it does not mean what
/// [`CAPTURES`] says.
///
/// **A type is coloured by the language.** Go draws a type reference teal, a
/// package olive and every built-in in a second orange; Java and Kotlin draw
/// a class reference plain, which is what the table already says. One mapping
/// cannot be both, so a grammar may have a few rows of its own here.
///
/// **Keyed by grammar and not by language**, because both engines are:
/// `syntax::spans` takes a grammar and the component's registry is one
/// entry per grammar. A language riding another's grammar — `ini` on
/// `toml` — therefore cannot carry paints, and a test holds it.
pub const PAINTS: &[(&str, &[(&str, Paint)])] = &[(
    "go",
    &[
        // A type reference: every `type_identifier` of the query, drawn
        // teal.
        ("type", Paint::Role(Role::TypeRef)),
        // A built-in constant and a built-in function call: `nil`,
        // `iota`, `len`, `make`.
        ("constant.builtin", Paint::Role(Role::Builtin)),
        ("function.builtin", Paint::Role(Role::Builtin)),
        // A package. **The capture is ours**: tree-sitter-go's query
        // does not name a package at all, so `grammars::PACKAGE_LINE` is
        // added to the text both engines read — see `editor`.
        ("module", Paint::Role(Role::Namespace)),
    ],
)];

/// The component slot a grammar's own paint is written into.
///
/// Three, and they are the component's unused names. A paint with no slot
/// could not reach a file in a tab at all.
pub fn slot_of(paint: Paint) -> Option<&'static str> {
    match paint {
        Paint::Role(Role::TypeRef) => Some("predictive"),
        Paint::Role(Role::Namespace) => Some("hint"),
        Paint::Role(Role::Builtin) => Some("primary"),
        _ => None,
    }
}

/// Extra patterns a grammar's query is given, because the query it ships
/// does not name what is coloured.
///
/// Go package names and Markdown quotations need explicit patterns.
/// `tree-sitter-go` 0.25 captures
/// fourteen things and a package name is not one of them, while a
/// package is painted olive wherever it stands.
pub const ADDED_PATTERNS: &[(&str, &str)] = &[
    ("go", "\n(package_identifier) @module\n"),
    ("markdown", "\n(block_quote) @string\n"),
];

/// What a capture is painted with in a grammar: its own row where it has
/// one, and the table's otherwise.
pub fn paint_of(grammar: &str, capture: &str) -> Option<Paint> {
    let rows = PAINTS
        .iter()
        .find(|(name, _)| *name == grammar)
        .map(|(_, rows)| *rows)
        .unwrap_or(&[]);
    paint_in(rows, capture).or_else(|| paint_of_capture(capture))
}

/// A grammar's own rows, read along the dots as the table is read.
fn paint_in(rows: &[(&str, Paint)], capture: &str) -> Option<Paint> {
    let mut name = capture;
    loop {
        if let Some((_, paint)) = rows.iter().find(|(known, _)| *known == name) {
            return Some(*paint);
        }
        let dot = name.rfind('.')?;
        name = &name[..dot];
    }
}

/// What a capture is painted with, falling back along the dots:
/// `function.method` finds `function`, and `keyword.control` finds
/// `keyword`. `None` where the table names nothing.
pub fn paint_of_capture(capture: &str) -> Option<Paint> {
    let mut name = capture;
    loop {
        if let Some((_, paint)) = CAPTURES.iter().find(|(known, _)| *known == name) {
            return Some(*paint);
        }
        let dot = name.rfind('.')?;
        name = &name[..dot];
    }
}

/// The query text both engines read for a grammar: what it ships, plus
/// whatever [`ADDED_PATTERNS`] adds, with the captures of [`PAINTS`] sent
/// to their slots and the rest through [`RENAMED`].
///
/// **One text, for the one-table rule.** The component reads it at
/// registration and `syntax::compiled` reads it to paint a diff; a
/// per-grammar colour written into one of them alone is a file that looks
/// one way in a tab and another in a diff, which is the drift that rule
/// exists to prevent.
pub fn query_for(grammar: &str, query: &str) -> String {
    let rows = PAINTS
        .iter()
        .find(|(name, _)| *name == grammar)
        .map(|(_, rows)| *rows)
        .unwrap_or(&[]);
    let mut text = renamed_captures_with(rows, query);
    if let Some((_, added)) = ADDED_PATTERNS.iter().find(|(name, _)| *name == grammar) {
        text.push_str(&renamed_captures_with(rows, added));
    }
    text
}

/// A query with the captures of [`RENAMED`] rewritten.
///
/// Pure, and deliberately not a regular expression: a capture is `@`
/// followed by a name, and a name is matched **along its dots and not by
/// its letters** — `@escaped` is not `@escape`, and a capture a predicate
/// refers to is left alone because it is not in the table.
///
/// **A string literal is copied through untouched.** css matches its
/// at-rules by name — `"@namespace" @keyword` — and scss matches fourteen
/// of them; those are tokens of the language and collide with rows of
/// `RENAMED` by spelling alone. Rewriting one leaves a query looking for
/// a token no stylesheet contains, and the at-rule goes grey with nothing
/// logged. A `;` comment is not skipped, because rewriting prose changes
/// nothing a parser reads.
pub fn renamed_captures(query: &str) -> String {
    renamed_captures_with(&[], query)
}

/// The same, with a grammar's own paints sent to their slots first.
fn renamed_captures_with(rows: &[(&str, Paint)], query: &str) -> String {
    let bytes = query.as_bytes();
    let mut out = String::with_capacity(query.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'"' {
            at = copy_string(query, at, &mut out);
            continue;
        }
        if bytes[at] != b'@' {
            let ch = query[at..].chars().next().expect("a boundary");
            out.push(ch);
            at += ch.len_utf8();
            continue;
        }
        let start = at + 1;
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'.'))
        {
            end += 1;
        }
        let capture = &query[start..end];
        out.push('@');
        // The grammar's own row first, and it wins: its slot is a name
        // `RENAMED` says nothing about. M8.10.
        match paint_in(rows, capture).and_then(slot_of) {
            Some(slot) => out.push_str(slot),
            None => out.push_str(&renamed(capture)),
        }
        at = end;
    }
    out
}

/// A string literal copied from `query` into `out`, and where it ended.
///
/// A backslash escapes whatever follows it, so `"\\@"` and `"a\"b"` end
/// where they really end rather than at the first quote after the
/// backslash.
fn copy_string(query: &str, opening: usize, out: &mut String) -> usize {
    let bytes = query.as_bytes();
    out.push('"');
    let mut at = opening + 1;
    while at < bytes.len() {
        let ch = query[at..].chars().next().expect("a boundary");
        out.push(ch);
        at += ch.len_utf8();
        match ch {
            '"' => return at,
            '\\' if at < bytes.len() => {
                let escaped = query[at..].chars().next().expect("a boundary");
                out.push(escaped);
                at += escaped.len_utf8();
            }
            _ => {}
        }
    }
    at
}

/// One capture name rewritten: the longest prefix of its dots that
/// [`RENAMED`] knows, with whatever followed it kept.
///
/// The prefix is what makes `@module.builtin` and `@character.special`
/// land somewhere. A rename that matched whole names only left them where
/// they were — with a root that exists in `RENAMED` and in no table
/// either engine reads, which is the same silence the rename was written
/// to end.
fn renamed(capture: &str) -> String {
    let mut name = capture;
    loop {
        if let Some((_, to)) = RENAMED.iter().find(|(from, _)| *from == name) {
            return format!("{to}{}", &capture[name.len()..]);
        }
        let Some(dot) = name.rfind('.') else {
            return capture.to_owned();
        };
        name = &name[..dot];
    }
}

/// How a file's lines end, remembered so that a save writes them back
/// the way they were read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// `\n`, and a file with no line break at all.
    Lf,
    /// `\r\n`.
    Crlf,
}

impl LineEnding {
    /// The text with this ending, from text whose lines end in `\n`.
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Lf => text.to_owned(),
            Self::Crlf => text.replace('\n', "\r\n"),
        }
    }
}

/// What a file's bytes turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    /// Text, with its lines ending in `\n` whatever they ended in on disk.
    Source {
        /// The text.
        text: String,
        /// How the lines ended on disk.
        line_ending: LineEnding,
    },
    /// Not text: a NUL in the first kilobytes — `git`'s test — or bytes
    /// that are not UTF-8.
    Binary,
}

impl Text {
    /// How many bytes are looked at for a NUL before the whole file is
    /// decoded.
    const SNIFF: usize = 8 * 1024;

    /// Decide what a file's bytes are.
    pub fn of(bytes: &[u8]) -> Self {
        if bytes[..bytes.len().min(Self::SNIFF)].contains(&0) {
            return Self::Binary;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Self::Binary;
        };
        let line_ending = if text.contains("\r\n") {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        };
        let text = match line_ending {
            LineEnding::Crlf => text.replace("\r\n", "\n"),
            LineEnding::Lf => text.to_owned(),
        };
        Self::Source { text, line_ending }
    }
}
