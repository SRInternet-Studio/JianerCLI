from __future__ import annotations
import json, os, shutil
from dataclasses import dataclass
from pathlib import Path
from .errors import ProjectError

@dataclass
class Project:
    root: Path
    kind: str
    config_path: Path | None = None
    plugins_dir: Path | None = None
    entry_script: Path | None = None
    venv_dir: Path | None = None
    display_name: str | None = None

    @classmethod
    def detect(cls, start: Path | None = None) -> "Project":
        current = (start or Path.cwd()).resolve()
        for directory in (current, *current.parents):
            config = directory / "config.json"
            if config.is_file():
                try:
                    data = json.loads(config.read_text(encoding="utf-8"))
                    bot_shape = isinstance(data, dict) and any(k in data for k in ("connections", "Connections", "protocol", "others"))
                except (OSError, json.JSONDecodeError):
                    bot_shape = False
                plugins = directory / "plugins"
                entry = next((directory / n for n in ("main.py", "bot.py") if (directory / n).is_file()), None)
                if bot_shape and (plugins.is_dir() or entry):
                    return cls(directory, "bot", config, plugins, entry, find_venv(directory), data.get("others", {}).get("bot_name") if isinstance(data, dict) else None)
            if (directory / "jianer" / "__init__.py").is_file():
                return cls(directory, "core", plugins_dir=directory / "plugins", venv_dir=find_venv(directory))
        raise ProjectError(f"未在 {current} 找到 JianerCore 项目或 Jianer_QQ_bot 实例")

    @classmethod
    def bot(cls, start: Path | None = None) -> "Project":
        project = cls.detect(start)
        if project.kind != "bot":
            raise ProjectError("该命令需要 Jianer_QQ_bot 实例")
        return project

    @property
    def python(self) -> str:
        candidates = []
        if self.venv_dir:
            candidates += [self.venv_dir / "bin/python", self.venv_dir / "bin/python3", self.venv_dir / "Scripts/python.exe"]
        if os.getenv("JIANER_PYTHON"):
            candidates.append(Path(os.environ["JIANER_PYTHON"]))
        candidates += [Path(shutil.which("python3") or "python3"), Path(shutil.which("python") or "python")]
        return str(next((p for p in candidates if p.is_file()), candidates[0]))

def find_venv(root: Path) -> Path | None:
    for name in (".venv", "venv", "env"):
        path = root / name
        if (path / "pyvenv.cfg").is_file() or (path / "bin").is_dir() or (path / "Scripts").is_dir():
            return path
    return None

def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as e:
        raise ProjectError(f"读取 {path} 失败：{e}") from e
    except json.JSONDecodeError as e:
        raise ProjectError(f"解析 {path} 失败：{e}") from e

def save_json(path: Path, data: dict) -> None:
    backup = path.with_suffix(path.suffix + ".bak")
    if path.exists(): shutil.copy2(path, backup)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    tmp.replace(path)
