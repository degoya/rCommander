"""The Rust that settings_reference.py reads: `const` items and `const fn` helpers, evaluated
without compiling.

Only what the settings schema writes is understood - string, integer and boolean literals,
paths to constants (in the module, through its `use` lines, in a child module, in another crate of
the workspace), struct and enum literals with field shorthand and `..base`, arrays, calls of a
`const fn` with a single expression for a body, `Duration::from_secs(n)` with `.as_secs()`, `as`
casts and the four integer operators. Anything else raises `RustError` naming where it stands, so
a construct the reader does not know stops the page instead of falling out of it.
"""

import re
from pathlib import Path

TOKEN = re.compile(
    r"""(?P<skip>\s+|//[^\n]*|/\*.*?\*/)
    |(?P<str>"(?:[^"\\]|\\.)*")
    |(?P<life>'[A-Za-z_]\w*(?!'))
    |(?P<num>\d[\d_]*(?:[iu](?:8|16|32|64|128|size))?)
    |(?P<ident>[A-Za-z_]\w*)
    |(?P<punct>::|\.\.|->|=>|[{}\[\](),:;&=*/+\-.<>!#?])""",
    re.S | re.X,
)
ITEM = r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?"
CONST = re.compile(ITEM + r"const[ \t]+([A-Z_][A-Z0-9_]*)[ \t]*:")
CONST_FN = re.compile(ITEM + r"const[ \t]+fn[ \t]+(\w+)[ \t]*\(")
USE = re.compile(ITEM + r"use[ \t]+")
INTEGERS = {"i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize"}
ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", '"': '"', "'": "'"}


class RustError(ValueError):
    """The source holds something this reader does not understand."""


def unescape(literal: str) -> str:
    def one(match: re.Match) -> str:
        text = match.group(1)
        if text.startswith("u{"):
            return chr(int(text[2:-1], 16))
        if text.startswith("\n"):
            return ""
        return ESCAPES[text]

    return re.sub(r"\\(u\{[0-9a-fA-F]+\}|\n\s*|.)", one, literal[1:-1])


class Lexer:
    """The tokens of `text` from `at` on, read as they are asked for."""

    def __init__(self, module: "Module", at: int):
        self.module, self.at, self.ahead = module, at, None
        self.start = at  # where the token `peek` gives starts

    def where(self) -> str:
        line = self.module.text.count("\n", 0, self.at) + 1
        return f"{self.module.name}:{line}"

    def peek(self) -> tuple[str, str]:
        if self.ahead is None:
            text = self.module.text
            while True:
                if self.at >= len(text):
                    self.ahead = ("end", "")
                    break
                match = TOKEN.match(text, self.at)
                if not match:
                    raise RustError(f"{self.where()}: cannot read {text[self.at : self.at + 20]!r}")
                self.at = match.end()
                if match.lastgroup != "skip":
                    self.ahead, self.start = (match.lastgroup, match.group()), match.start()
                    break
        return self.ahead

    def next(self) -> tuple[str, str]:
        token = self.peek()
        self.ahead = None
        return token

    def take(self, text: str) -> bool:
        if self.peek()[1] == text:
            self.next()
            return True
        return False

    def expect(self, text: str) -> None:
        kind, got = self.next()
        if got != text:
            raise RustError(f"{self.where()}: expected {text!r}, found {got or kind!r}")

    def skip_until(self, *stops: str) -> None:
        """Past a type: every token up to one of `stops` outside brackets."""
        depth = 0
        while True:
            _, text = self.peek()
            if depth == 0 and text in stops:
                return
            if text == "":
                raise RustError(f"{self.where()}: the source ends inside a type")
            depth += text in "([{<"
            depth -= text in ")]}>"
            self.next()


class Module:
    """One Rust file: its constants, its `const fn`s and its `use` lines, found by their place."""

    def __init__(self, world: "World", path: Path, crate: str, parent: "Module | None" = None):
        self.world, self.path, self.crate, self.parent = world, path, crate, parent
        self.name = str(path.relative_to(world.root))
        self.text = path.read_text(encoding="utf-8")
        self.consts = self.items(CONST)
        self.fns = self.items(CONST_FN)
        self.uses: dict[str, list[str]] = {}
        for match in USE.finditer(self.text):
            try:
                self.use_tree(Lexer(self, match.end()), [])
            except RustError:
                continue  # a name it would bring that is needed fails as unresolved instead
        self.values: dict[str, object] = {}
        folder = path.with_suffix("")
        self.children = {child.stem: child for child in folder.glob("*.rs")} if folder.is_dir() else {}

    def items(self, pattern: re.Pattern) -> dict[str, list[int]]:
        found: dict[str, list[int]] = {}
        for match in pattern.finditer(self.text):
            found.setdefault(match.group(1), []).append(match.end())
        return found

    def place(self, items: dict[str, list[int]], name: str) -> int:
        """Where the one definition of `name` starts; two (behind `cfg`, say) are refused."""
        if len(items[name]) != 1:
            raise RustError(f"{self.name}: {name} is defined {len(items[name])} times")
        return items[name][0]

    def use_tree(self, lexer: Lexer, prefix: list[str]) -> None:
        path = list(prefix)
        while True:
            if lexer.take("{"):
                while not lexer.take("}"):
                    self.use_tree(lexer, path)
                    lexer.take(",")
                return
            kind, text = lexer.next()
            if text == "*":
                return
            if kind != "ident":
                raise RustError(f"{lexer.where()}: a use line this reader does not know")
            path.append(text)
            if not lexer.take("::"):
                break
        name = lexer.next()[1] if lexer.take("as") else path[-1]
        self.uses[name] = path

    def const(self, name: str) -> object:
        if name not in self.values:
            lexer = Lexer(self, self.place(self.consts, name))
            lexer.skip_until("=")
            lexer.expect("=")
            self.values[name] = Evaluator(self, {}).expr(lexer)
            lexer.expect(";")
        return self.values[name]

    def call(self, name: str, args: list) -> object:
        lexer = Lexer(self, self.place(self.fns, name))
        params = []
        while not lexer.take(")"):
            kind, param = lexer.next()
            if kind != "ident":
                raise RustError(f"{lexer.where()}: a parameter this reader does not know")
            lexer.expect(":")
            lexer.skip_until(",", ")")
            lexer.take(",")
            params.append(param)
        if len(params) != len(args):
            raise RustError(f"{lexer.where()}: {name} takes {len(params)} arguments, given {len(args)}")
        lexer.expect("->")
        lexer.skip_until("{")
        lexer.expect("{")
        value = Evaluator(self, dict(zip(params, args))).expr(lexer)
        lexer.expect("}")
        return value


class World:
    """The workspace's files as modules, each read once."""

    def __init__(self, root: Path):
        self.root = root
        self.modules: dict[Path, Module] = {}

    def module(self, path: Path, crate: str, parent: Module | None = None) -> Module:
        if path not in self.modules:
            self.modules[path] = Module(self, path, crate, parent)
        return self.modules[path]

    def in_crate(self, crate: str, name: str, where: str) -> tuple[Module, str]:
        """The one definition of the constant or `const fn` `name` in `crate`'s sources."""
        source = self.root / "crates" / crate / "src"
        if not source.is_dir():
            raise RustError(f"{where}: no crate {crate}")
        hits = []
        for path in sorted(source.rglob("*.rs")):
            if path.stem.endswith("_tests"):
                continue
            text = path.read_text(encoding="utf-8")
            if re.search(ITEM + r"const[ \t]+(?:fn[ \t]+)?" + re.escape(name) + r"\b", text):
                hits.append(path)
        if len(hits) != 1:
            raise RustError(f"{where}: {name} is defined {len(hits)} times in crates/{crate}/src")
        return self.module(hits[0], crate), name


def resolve(module: Module, path: list[str], where: str) -> tuple[Module, str]:
    """The module that defines the last segment of `path`, as seen from `module`."""
    head, rest = path[0], path[1:]
    if not rest:
        if head in module.consts or head in module.fns:
            return module, head
        if head in module.uses:
            return resolve(module, module.uses[head], where)
        raise RustError(f"{where}: cannot resolve {head}")
    if head == "self":
        return resolve(module, rest, where)
    if head == "super" and module.parent:
        return resolve(module.parent, rest, where)
    if head == "crate":
        return module.world.in_crate(module.crate, path[-1], where)
    if head in module.children:
        return resolve(module.world.module(module.children[head], module.crate, module), rest, where)
    if head in module.uses:
        return resolve(module, module.uses[head] + rest, where)
    if head.startswith("rc_") or head == "rcommander":
        return module.world.in_crate(head.replace("_", "-"), path[-1], where)
    raise RustError(f"{where}: cannot resolve {'::'.join(path)}")


class Evaluator:
    """Evaluates one expression in `module`, with `env` the arguments of a `const fn`."""

    def __init__(self, module: Module, env: dict):
        self.module, self.env = module, env

    def expr(self, lexer: Lexer) -> object:
        value = self.term(lexer)
        while lexer.peek()[1] in ("+", "-"):
            op = lexer.next()[1]
            right = self.term(lexer)
            value = self.integer(value, lexer) + (1 if op == "+" else -1) * self.integer(right, lexer)
        return value

    def term(self, lexer: Lexer) -> object:
        value = self.cast(lexer)
        while lexer.peek()[1] in ("*", "/"):
            op = lexer.next()[1]
            left, right = self.integer(value, lexer), self.integer(self.cast(lexer), lexer)
            if op == "*":
                value = left * right
            elif right == 0:
                raise RustError(f"{lexer.where()}: division by zero")
            else:
                value = abs(left) // abs(right) * (1 if (left < 0) == (right < 0) else -1)
        return value

    def cast(self, lexer: Lexer) -> object:
        value = self.postfix(lexer)
        while lexer.take("as"):
            target = lexer.next()[1]
            if target not in INTEGERS:
                raise RustError(f"{lexer.where()}: a cast to {target} this reader does not know")
            value = self.integer(value, lexer)
        return value

    def postfix(self, lexer: Lexer) -> object:
        value = self.unary(lexer)
        while lexer.take("."):
            method = lexer.next()[1]
            lexer.expect("(")
            lexer.expect(")")
            if method != "as_secs" or not (isinstance(value, tuple) and value[0] == "Duration"):
                raise RustError(f"{lexer.where()}: a method .{method}() this reader does not know")
            value = value[1]
        return value

    def unary(self, lexer: Lexer) -> object:
        if lexer.take("-"):
            return -self.integer(self.unary(lexer), lexer)
        if lexer.take("&"):
            return self.unary(lexer)
        return self.primary(lexer)

    @staticmethod
    def integer(value: object, lexer: Lexer) -> int:
        if isinstance(value, bool) or not isinstance(value, int):
            raise RustError(f"{lexer.where()}: {value!r} is not an integer")
        return value

    def primary(self, lexer: Lexer) -> object:
        kind, text = lexer.next()
        if kind == "str":
            return unescape(text)
        if kind == "num":
            return int(re.sub(r"[iu]\w+$", "", text).replace("_", ""))
        if text in ("true", "false"):
            return text == "true"
        if text == "[":
            items = []
            while not lexer.take("]"):
                items.append(self.expr(lexer))
                if not lexer.take(","):
                    lexer.expect("]")
                    break
            return items
        if text == "(":
            value = self.expr(lexer)
            lexer.expect(")")
            return value
        if kind != "ident":
            raise RustError(f"{lexer.where()}: an expression this reader does not know at {text!r}")
        path = [text]
        while lexer.take("::"):
            path.append(lexer.next()[1])
        if lexer.peek()[1] == "{":
            return self.struct(lexer, "::".join(path))
        if lexer.take("("):
            args = []
            while not lexer.take(")"):
                args.append(self.expr(lexer))
                lexer.take(",")
            if path == ["Duration", "from_secs"] and len(args) == 1:
                return ("Duration", self.integer(args[0], lexer))
            module, name = resolve(self.module, path, lexer.where())
            if name not in module.fns:
                raise RustError(f"{lexer.where()}: {name} is no const fn")
            return module.call(name, args)
        if len(path) == 1 and text in self.env:
            return self.env[text]
        if len(path) == 2 and path[0][:1].isupper() and path[1][:1].isupper():
            return ("enum", path[0], path[1])
        module, name = resolve(self.module, path, lexer.where())
        if name not in module.consts:
            raise RustError(f"{lexer.where()}: {name} is no constant")
        return module.const(name)

    def struct(self, lexer: Lexer, type_name: str) -> dict:
        lexer.expect("{")
        fields: dict = {}
        base: dict = {}
        while not lexer.take("}"):
            if lexer.take(".."):
                base = self.expr(lexer)
                if not isinstance(base, dict) or base.get("__type") != type_name:
                    raise RustError(f"{lexer.where()}: ..base is not a {type_name}")
                lexer.expect("}")
                break
            kind, field = lexer.next()
            if kind != "ident":
                raise RustError(f"{lexer.where()}: a field this reader does not know at {field!r}")
            if lexer.take(":"):
                fields[field] = self.expr(lexer)
            elif field in self.env:
                fields[field] = self.env[field]
            else:
                raise RustError(f"{lexer.where()}: the shorthand field {field} has no value")
            lexer.take(",")
        return {**base, **fields, "__type": type_name}
