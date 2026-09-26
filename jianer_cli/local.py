from __future__ import annotations
import ast, json
from dataclasses import dataclass, asdict
from pathlib import Path
from .errors import PluginError
PREFIX = "jianerbot-plugin-"
BUILTINS = {"jianerbot-plugin-alconna"}

@dataclass
class Meta:
    name: str
    description: str = ""
    usage: str = ""
    requires: list[str] = None
    def __post_init__(self): self.requires = sorted(self.requires or [])

@dataclass
class LocalPlugin:
    id: str | None
    path: Path
    entry: Path
    kind: str
    disabled: bool
    meta: Meta | None
    version: str | None
    error: str | None = None
    def json(self):
        d = asdict(self); d["path"] = str(self.path); d["entry"] = str(self.entry); return d

def parse_meta_source(source: str) -> Meta | None:
    try: tree = ast.parse(source)
    except SyntaxError: return None
    for node in tree.body:
        if not isinstance(node, ast.Assign): continue
        if not any(isinstance(t, ast.Name) and t.id == "__plugin_meta__" for t in node.targets): continue
        call = node.value
        if not isinstance(call, ast.Call): return None
        values = {}
        for kw in call.keywords:
            if kw.arg is None: continue
            try: values[kw.arg] = ast.literal_eval(kw.value)
            except (ValueError, TypeError): pass
        name = values.get("name")
        if not isinstance(name, str): return None
        return Meta(name, str(values.get("description", "")), str(values.get("usage", "")), list(values.get("requires", [])))
    return None

def parse_meta(path: Path) -> Meta | None:
    try: return parse_meta_source(path.read_text(encoding="utf-8"))
    except OSError: return None

def valid_id(value: str) -> bool:
    rest = value.removeprefix(PREFIX)
    return value.startswith(PREFIX) and bool(rest) and all(part and all(c.isascii() and (c.islower() or c.isdigit()) for c in part) for part in rest.split("-"))

def scan_plugins(directory: Path) -> list[LocalPlugin]:
    if not directory.is_dir(): return []
    result = []
    for path in sorted(directory.iterdir()):
        if path.name == "__pycache__" or path.name.startswith(".") or path.name.endswith(".market.json") or path.name == "market.json": continue
        disabled = path.name.startswith("d_")
        if path.is_dir():
            entry = path / "setup.py"
            if not entry.is_file(): continue
            kind = "package"
        elif path.suffix in (".py", ".pyw"):
            entry, kind = path, "single-file"
        else: continue
        meta = parse_meta(entry)
        sidecar = path / "market.json" if path.is_dir() else path.with_name(path.stem + ".market.json")
        version = None
        try: version = json.loads(sidecar.read_text(encoding="utf-8")).get("version")
        except (OSError, json.JSONDecodeError): pass
        error = None if meta and valid_id(meta.name) else "缺少或无效的 __plugin_meta__"
        result.append(LocalPlugin(meta.name if meta else None, path, entry, kind, disabled, meta, version, error))
    return result

def match_local(plugins, query):
    q = query.lower()
    exact = [p for p in plugins if (p.id or "").lower() == q or p.path.stem.lower() == q or p.path.name.lower() == q]
    return exact or [p for p in plugins if q in (p.id or "").lower() or q in p.path.name.lower()]
