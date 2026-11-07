//! The two languages the viewer reads, named by the file's extension - never guessed from its
//! content. Both describe a schema - keywords, types, fields, blocks - which is why one plugin
//! reads both; their grammars share little else, so each has its own pass over code
//! (`lexer/proto.rs`, `lexer/graphql.rs`).

/// A schema language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// Protocol Buffers: `.proto`.
    Proto,
    /// GraphQL: `.graphql`, `.gql`, and the schema files some servers name `.graphqls`.
    GraphQl,
}

impl Lang {
    /// The language of a file named `name`, by its extension in any case (`manifest.toml`'s
    /// `[match] extensions`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let (_, extension) = name.rsplit_once('.')?;
        Some(match extension.to_ascii_lowercase().as_str() {
            "proto" => Self::Proto,
            "graphql" | "gql" | "graphqls" => Self::GraphQl,
            _ => return None,
        })
    }
}
