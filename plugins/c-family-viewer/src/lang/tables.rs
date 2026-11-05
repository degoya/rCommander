//! The tables of each language: keywords, types and constants, sorted by their bytes so
//! [`super::Lang::classify`] finds a word by binary search (`lang_tests.rs` holds them sorted);
//! one table may serve several languages.

/// C23's keywords but its types and constants.
#[rustfmt::skip]
pub(super) const C_KEYWORDS: &[&[u8]] = &[
    b"_Alignas", b"_Alignof", b"_Atomic", b"_Generic", b"_Noreturn", b"_Static_assert",
    b"_Thread_local", b"alignas", b"alignof", b"auto", b"break", b"case", b"const", b"constexpr",
    b"continue", b"default", b"do", b"else", b"enum", b"extern", b"for", b"goto", b"if", b"inline",
    b"register", b"restrict", b"return", b"sizeof", b"static", b"static_assert", b"struct",
    b"switch", b"thread_local", b"typedef", b"typeof", b"typeof_unqual", b"union", b"volatile",
    b"while",
];

/// C's types, and the typedefs of `<stddef.h>`, `<stdint.h>` and `<stdio.h>` every file uses.
#[rustfmt::skip]
pub(super) const C_TYPES: &[&[u8]] = &[
    b"FILE", b"_BitInt", b"_Bool", b"_Complex", b"_Decimal128", b"_Decimal32", b"_Decimal64",
    b"_Imaginary", b"bool", b"char", b"char16_t", b"char32_t", b"char8_t", b"double", b"float",
    b"int", b"int16_t", b"int32_t", b"int64_t", b"int8_t", b"intmax_t", b"intptr_t", b"long",
    b"ptrdiff_t", b"short", b"signed", b"size_t", b"ssize_t", b"uint16_t", b"uint32_t",
    b"uint64_t", b"uint8_t", b"uintmax_t", b"uintptr_t", b"unsigned", b"void", b"wchar_t",
];

/// C's constants, `NULL` among them.
#[rustfmt::skip]
pub(super) const C_LITERALS: &[&[u8]] = &[
    b"NULL", b"false", b"nullptr", b"true",
];

/// C++23's keywords and alternative tokens, and C's, since a `.h` may be either.
#[rustfmt::skip]
pub(super) const CPP_KEYWORDS: &[&[u8]] = &[
    b"_Alignas", b"_Alignof", b"_Atomic", b"_Generic", b"_Noreturn", b"_Static_assert",
    b"_Thread_local", b"alignas", b"alignof", b"and", b"and_eq", b"asm", b"auto", b"bitand",
    b"bitor", b"break", b"case", b"catch", b"class", b"co_await", b"co_return", b"co_yield",
    b"compl", b"concept", b"const", b"const_cast", b"consteval", b"constexpr", b"constinit",
    b"continue", b"decltype", b"default", b"delete", b"do", b"dynamic_cast", b"else", b"enum",
    b"explicit", b"export", b"extern", b"final", b"for", b"friend", b"goto", b"if", b"import",
    b"inline", b"module", b"mutable", b"namespace", b"new", b"noexcept", b"not", b"not_eq",
    b"operator", b"or", b"or_eq", b"override", b"private", b"protected", b"public", b"register",
    b"reinterpret_cast", b"requires", b"restrict", b"return", b"sizeof", b"static",
    b"static_assert", b"static_cast", b"struct", b"switch", b"template", b"this", b"thread_local",
    b"throw", b"try", b"typedef", b"typeid", b"typename", b"typeof", b"union", b"using",
    b"virtual", b"volatile", b"while", b"xor", b"xor_eq",
];

/// C#'s keywords and the contextual ones a reader expects coloured (`var`, `async`, ...).
#[rustfmt::skip]
pub(super) const CSHARP_KEYWORDS: &[&[u8]] = &[
    b"abstract", b"and", b"as", b"async", b"await", b"base", b"break", b"case", b"catch",
    b"checked", b"class", b"const", b"continue", b"default", b"delegate", b"do", b"else", b"enum",
    b"event", b"explicit", b"extern", b"finally", b"fixed", b"for", b"foreach", b"get", b"global",
    b"goto", b"if", b"implicit", b"in", b"init", b"interface", b"internal", b"is", b"lock",
    b"nameof", b"namespace", b"new", b"not", b"operator", b"or", b"out", b"override", b"params",
    b"partial", b"private", b"protected", b"public", b"readonly", b"record", b"ref", b"required",
    b"return", b"sealed", b"set", b"sizeof", b"stackalloc", b"static", b"struct", b"switch",
    b"this", b"throw", b"try", b"typeof", b"unchecked", b"unsafe", b"using", b"var", b"virtual",
    b"volatile", b"when", b"where", b"while", b"with", b"yield",
];

/// C#'s built-in types.
#[rustfmt::skip]
pub(super) const CSHARP_TYPES: &[&[u8]] = &[
    b"bool", b"byte", b"char", b"decimal", b"double", b"dynamic", b"float", b"int", b"long",
    b"nint", b"nuint", b"object", b"sbyte", b"short", b"string", b"uint", b"ulong", b"ushort",
    b"void",
];

/// Java's keywords and contextual keywords (`var`, `record`, `sealed`, `yield`, ...).
#[rustfmt::skip]
pub(super) const JAVA_KEYWORDS: &[&[u8]] = &[
    b"abstract", b"assert", b"break", b"case", b"catch", b"class", b"const", b"continue",
    b"default", b"do", b"else", b"enum", b"extends", b"final", b"finally", b"for", b"goto", b"if",
    b"implements", b"import", b"instanceof", b"interface", b"native", b"new", b"package",
    b"permits", b"private", b"protected", b"public", b"record", b"return", b"sealed", b"static",
    b"strictfp", b"super", b"switch", b"synchronized", b"this", b"throw", b"throws", b"transient",
    b"try", b"var", b"volatile", b"when", b"while", b"yield",
];

/// Java's primitive types, and `String` and `Object` of `java.lang`.
#[rustfmt::skip]
pub(super) const JAVA_TYPES: &[&[u8]] = &[
    b"Object", b"String", b"boolean", b"byte", b"char", b"double", b"float", b"int", b"long",
    b"short", b"void",
];

/// Kotlin's hard keywords, and the soft keywords and modifiers a reader expects coloured.
#[rustfmt::skip]
pub(super) const KOTLIN_KEYWORDS: &[&[u8]] = &[
    b"abstract", b"actual", b"annotation", b"as", b"break", b"by", b"catch", b"class",
    b"companion", b"const", b"constructor", b"continue", b"crossinline", b"data", b"do", b"else",
    b"enum", b"expect", b"external", b"final", b"finally", b"for", b"fun", b"get", b"if",
    b"import", b"in", b"infix", b"init", b"inline", b"inner", b"interface", b"internal", b"is",
    b"lateinit", b"noinline", b"object", b"open", b"operator", b"out", b"override", b"package",
    b"private", b"protected", b"public", b"reified", b"return", b"sealed", b"set", b"super",
    b"suspend", b"tailrec", b"this", b"throw", b"try", b"typealias", b"typeof", b"val", b"var",
    b"vararg", b"when", b"where", b"while",
];

/// Kotlin's basic types.
#[rustfmt::skip]
pub(super) const KOTLIN_TYPES: &[&[u8]] = &[
    b"Any", b"Array", b"Boolean", b"Byte", b"Char", b"Double", b"Float", b"Int", b"Long",
    b"Nothing", b"Short", b"String", b"UByte", b"UInt", b"ULong", b"UShort", b"Unit",
];

/// `true`, `false` and `null`: C#, Java and Kotlin.
#[rustfmt::skip]
pub(super) const NULL_LITERALS: &[&[u8]] = &[
    b"false", b"null", b"true",
];

/// Go's keywords.
#[rustfmt::skip]
pub(super) const GO_KEYWORDS: &[&[u8]] = &[
    b"break", b"case", b"chan", b"const", b"continue", b"default", b"defer", b"else",
    b"fallthrough", b"for", b"func", b"go", b"goto", b"if", b"import", b"interface", b"map",
    b"package", b"range", b"return", b"select", b"struct", b"switch", b"type", b"var",
];

/// Go's predeclared types.
#[rustfmt::skip]
pub(super) const GO_TYPES: &[&[u8]] = &[
    b"any", b"bool", b"byte", b"comparable", b"complex128", b"complex64", b"error", b"float32",
    b"float64", b"int", b"int16", b"int32", b"int64", b"int8", b"rune", b"string", b"uint",
    b"uint16", b"uint32", b"uint64", b"uint8", b"uintptr",
];

/// Go's predeclared constants and `nil`.
#[rustfmt::skip]
pub(super) const GO_LITERALS: &[&[u8]] = &[
    b"false", b"iota", b"nil", b"true",
];

/// Rust's strict and reserved keywords, and `union`.
#[rustfmt::skip]
pub(super) const RUST_KEYWORDS: &[&[u8]] = &[
    b"Self", b"abstract", b"as", b"async", b"await", b"become", b"box", b"break", b"const",
    b"continue", b"crate", b"do", b"dyn", b"else", b"enum", b"extern", b"final", b"fn", b"for",
    b"if", b"impl", b"in", b"let", b"loop", b"macro", b"match", b"mod", b"move", b"mut",
    b"override", b"priv", b"pub", b"ref", b"return", b"self", b"static", b"struct", b"super",
    b"trait", b"try", b"type", b"typeof", b"union", b"unsafe", b"unsized", b"use", b"virtual",
    b"where", b"while", b"yield",
];

/// Rust's primitive types and the prelude's `Box`, `Option`, `Result`, `String` and `Vec`.
#[rustfmt::skip]
pub(super) const RUST_TYPES: &[&[u8]] = &[
    b"Box", b"Option", b"Result", b"String", b"Vec", b"bool", b"char", b"f32", b"f64", b"i128",
    b"i16", b"i32", b"i64", b"i8", b"isize", b"str", b"u128", b"u16", b"u32", b"u64", b"u8",
    b"usize",
];

/// `true` and `false`.
#[rustfmt::skip]
pub(super) const RUST_LITERALS: &[&[u8]] = &[
    b"false", b"true",
];
