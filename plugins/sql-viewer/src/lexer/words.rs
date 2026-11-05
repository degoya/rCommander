//! The words the SQL lexer knows, one table for every dialect: ANSI SQL, PostgreSQL, MySQL and
//! MariaDB, SQLite and T-SQL. No dialect parser - a word that is a keyword in one dialect is one
//! in every file. Each table is sorted and upper case, so a word is found by binary search after
//! it is upper-cased (`lexer_tests.rs` holds the tables to that).

/// The keywords: statements, clauses, operators spelled as words, the procedural words of
/// PL/pgSQL, MySQL's procedures and T-SQL.
#[rustfmt::skip]
pub(crate) const KEYWORDS: &[&str] = &[
    "ABORT", "ACTION", "ADD", "AFTER", "AGGREGATE", "ALL", "ALTER", "ALWAYS", "ANALYZE", "AND",
    "ANY", "APPLY", "AS", "ASC", "ATTACH", "AUTHORIZATION", "AUTOINCREMENT", "AUTO_INCREMENT",
    "BEFORE", "BEGIN", "BETWEEN", "BREAK", "BROWSE", "BULK", "BY", "CALL", "CASCADE", "CASE",
    "CAST", "CATCH", "CHECK", "CHECKPOINT", "CLOSE", "CLUSTER", "CLUSTERED", "COLLATE", "COLUMN",
    "COMMENT", "COMMIT", "CONCURRENTLY", "CONFLICT", "CONSTRAINT", "CONTINUE", "CONVERT", "COPY",
    "CREATE", "CROSS", "CURRENT", "CURSOR", "DATABASE", "DEALLOCATE", "DECLARE", "DEFAULT",
    "DEFERRABLE", "DEFERRED", "DEFINER", "DELAYED", "DELETE", "DELIMITER", "DENY", "DESC",
    "DESCRIBE", "DETACH", "DISTINCT", "DISTINCTROW", "DO", "DOMAIN", "DROP", "DUPLICATE", "EACH",
    "ELSE", "ELSEIF", "ELSIF", "ENCRYPTION", "END", "ENGINE", "ESCAPE", "EXCEPT", "EXCEPTION",
    "EXCLUDE", "EXCLUSIVE", "EXEC", "EXECUTE", "EXISTS", "EXIT", "EXPLAIN", "EXTENSION", "FAIL",
    "FETCH", "FILTER", "FIRST", "FOLLOWING", "FOR", "FOREIGN", "FROM", "FULL", "FULLTEXT",
    "FUNCTION", "GENERATED", "GLOB", "GO", "GOTO", "GRANT", "GROUP", "GROUPS", "HANDLER",
    "HAVING", "IDENTITY", "IF", "IGNORE", "ILIKE", "IMMEDIATE", "IMMUTABLE", "IN", "INCLUDE",
    "INDEX", "INDEXED", "INHERITS", "INITIALLY", "INNER", "INOUT", "INSERT", "INSTEAD",
    "INTERSECT", "INTO", "INVOKER", "IS", "ISNULL", "ITERATE", "JOIN", "KEY", "KEYS", "KILL",
    "LANGUAGE", "LAST", "LATERAL", "LEADING", "LEAVE", "LEFT", "LIKE", "LIMIT", "LISTEN",
    "LOCK", "LOOP", "MATCH", "MATERIALIZED", "MERGE", "MODIFY", "NATURAL", "NO", "NOCHECK",
    "NOCOUNT", "NONCLUSTERED", "NOT", "NOTHING", "NOTIFY", "NOTNULL", "NOWAIT", "NULLS", "OF",
    "OFF", "OFFSET", "ON", "ONLY", "OPEN", "OPTION", "OR", "ORDER", "OTHERS", "OUT", "OUTER",
    "OUTPUT", "OVER", "OWNER", "PARTITION", "PERFORM", "PIVOT", "PLAN", "POLICY", "PRAGMA",
    "PRECEDING", "PREPARE", "PRIMARY", "PRINT", "PRIVILEGES", "PROC", "PROCEDURE", "PUBLICATION",
    "QUERY", "RAISE", "RAISERROR", "RANGE", "READ", "RECURSIVE", "REFERENCES", "REFRESH",
    "REINDEX", "RELEASE", "RENAME", "REPEAT", "REPLACE", "RESTRICT", "RETURN", "RETURNING",
    "RETURNS", "REVOKE", "RIGHT", "ROLE", "ROLLBACK", "ROLLUP", "ROW", "ROWS", "RULE",
    "SAVEPOINT", "SCHEMA", "SECURITY", "SELECT", "SEQUENCE", "SESSION", "SET", "SETOF", "SHARE",
    "SHOW", "SIMILAR", "SOME", "SQL", "STABLE", "START", "STATEMENT", "STORED", "STRICT",
    "SUBSCRIPTION", "TABLE", "TABLESPACE", "TEMP", "TEMPORARY", "THEN", "THROW", "TIES", "TO",
    "TOP", "TRAN", "TRANSACTION", "TRIGGER", "TRUNCATE", "TRY", "TYPE", "UNBOUNDED", "UNION",
    "UNIQUE", "UNLOCK", "UNPIVOT", "UNSIGNED", "UNTIL", "UPDATE", "USE", "USING", "VACUUM",
    "VALUES", "VARIADIC", "VIEW", "VIRTUAL", "VOLATILE", "WAITFOR", "WHEN", "WHERE", "WHILE",
    "WINDOW", "WITH", "WITHOUT", "WORK", "ZEROFILL",
];

/// The built-in types.
#[rustfmt::skip]
pub(crate) const TYPES: &[&str] = &[
    "BIGINT", "BIGSERIAL", "BINARY", "BIT", "BLOB", "BOOL", "BOOLEAN", "BOX", "BYTEA", "CHAR",
    "CHARACTER", "CIDR", "CIRCLE", "CLOB", "DATE", "DATERANGE", "DATETIME", "DATETIME2",
    "DATETIMEOFFSET", "DEC", "DECIMAL", "DOUBLE", "ENUM", "FLOAT", "FLOAT4", "FLOAT8",
    "GEOGRAPHY", "GEOMETRY", "HIERARCHYID", "IMAGE", "INET", "INT", "INT2", "INT4", "INT4RANGE",
    "INT8", "INT8RANGE", "INTEGER", "INTERVAL", "JSON", "JSONB", "LINE", "LONGBLOB", "LONGTEXT",
    "LSEG", "MACADDR", "MEDIUMBLOB", "MEDIUMINT", "MEDIUMTEXT", "MONEY", "NCHAR", "NTEXT",
    "NUMERIC", "NUMRANGE", "NVARCHAR", "OID", "PATH", "POINT", "POLYGON", "PRECISION", "REAL",
    "REGCLASS", "ROWVERSION", "SERIAL", "SMALLDATETIME", "SMALLINT", "SMALLMONEY", "SMALLSERIAL",
    "SQL_VARIANT", "TEXT", "TIME", "TIMESTAMP", "TIMESTAMPTZ", "TIMETZ", "TINYBLOB", "TINYINT",
    "TINYTEXT", "TSQUERY", "TSRANGE", "TSTZRANGE", "TSVECTOR", "UNIQUEIDENTIFIER", "UUID",
    "VARBINARY", "VARCHAR", "VARCHAR2", "VARYING", "XML",
];

/// The constants spelled as words.
pub(crate) const LITERALS: &[&str] = &["FALSE", "NULL", "TRUE", "UNKNOWN"];

/// What a word is, by the tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Word {
    Keyword,
    Type,
    Literal,
    Name,
}

/// The longest word in the tables, and so the longest worth upper-casing.
const LONGEST: usize = 16;

/// The kind of `word` (ASCII letters, digits, `_`; any case).
pub(crate) fn classify(word: &[u8]) -> Word {
    if word.len() > LONGEST {
        return Word::Name;
    }
    let mut upper = [0u8; LONGEST];
    let upper = &mut upper[..word.len()];
    upper.copy_from_slice(word);
    upper.make_ascii_uppercase();
    let Ok(upper) = std::str::from_utf8(upper) else {
        return Word::Name;
    };
    let found = |table: &[&str]| table.binary_search(&upper).is_ok();
    if found(LITERALS) {
        Word::Literal
    } else if found(TYPES) {
        Word::Type
    } else if found(KEYWORDS) {
        Word::Keyword
    } else {
        Word::Name
    }
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
