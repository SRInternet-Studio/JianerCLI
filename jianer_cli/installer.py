from __future__ import annotations
import json, shutil, subprocess, tempfile, zipfile
from pathlib import Path
from .errors import PluginError
from .local import BUILTINS, scan_plugins

def safe_extract(archive: Path, dest: Path):
    with zipfile.ZipFile(archive) as z:
        for info in z.infolist():
            target=(dest/info.filename).resolve()
            if not str(target).startswith(str(dest.resolve())+"/") and target != dest.resolve(): raise PluginError(f"插件包包含不安全路径：{info.filename}")
            z.extract(info,dest)

def dependency_order(root, lookup, installed):
    result=[]; visiting=set(); visited=set()
    def visit(pid):
        if pid in BUILTINS or pid in installed or pid in visited: return
        if pid in visiting: raise PluginError(f"插件依赖存在环：{pid}")
        plugin=lookup.get(pid)
        if plugin is None: raise PluginError(f"依赖插件不存在：{pid}")
        visiting.add(pid)
        for dep in plugin.requires: visit(dep)
        visiting.remove(pid); visited.add(pid); result.append(pid)
    visit(root); return result

def install(market, index, plugin, project, upgrade=False, all_plugins=False, dry_run=False):
    installed={p.id for p in scan_plugins(project.plugins_dir) if p.id}
    if upgrade: installed.discard(plugin.id)
    _, plugins=market._parse(json.dumps(index))
    lookup={p.id:p for p in plugins}
    order=[p.id for p in plugins if not p.deprecated] if all_plugins else dependency_order(plugin.id,lookup,installed)
    for pid in order:
        p=lookup[pid]
        if dry_run:
            print(f"将安装 {p.id} {p.version}，PyPI 依赖：{', '.join(p.pip_dependencies) or '无'}"); continue
        archive=market.download(index,p)
        with tempfile.TemporaryDirectory(prefix="jianer-install-") as temp:
            stage=Path(temp); safe_extract(archive,stage)
            roots=list(stage.iterdir())
            if not roots: raise PluginError(f"插件包为空：{p.id}")
            for root in roots:
                target=project.plugins_dir/root.name
                if target.exists():
                    backup=target.with_name(target.name+".jianer.bak")
                    if backup.exists(): shutil.rmtree(backup) if backup.is_dir() else backup.unlink()
                    target.rename(backup)
                root.rename(target)
                sidecar=target/"market.json" if target.is_dir() else target.with_name(target.stem+".market.json")
                sidecar.write_text(json.dumps({"id":p.id,"version":p.version,"release":p.release["tag"]},ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        if p.pip_dependencies:
            command=[project.python,"-m","pip","install",*p.pip_dependencies]
            print(f"为 {p.id} 安装 PyPI 依赖：{', '.join(p.pip_dependencies)}")
            subprocess.run(command,cwd=project.root,check=True)
        print(f"已安装 {p.id} {p.version}")
