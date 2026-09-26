from __future__ import annotations
import argparse, json, shutil, subprocess, sys
from pathlib import Path
from . import __version__
from .config import load_user, save_user
from .errors import JianerError, ProjectError
from .installer import install
from .local import match_local, scan_plugins
from .market import Market, find_plugin
from .project import Project, read_json, save_json
from .ui import emit, table
from .update import CHANNELS, configure, upgrade

def parser():
    p=argparse.ArgumentParser(prog="jianer-cli",description="管理 JianerCore / Jianer_QQ_bot 实例")
    p.add_argument("--version",action="version",version=__version__); p.add_argument("--json",action="store_true"); p.add_argument("--market-url")
    sub=p.add_subparsers(dest="command", metavar="COMMAND")
    c=sub.add_parser("create", help="创建 JianerCore 或 Bot 项目"); c.add_argument("kind",choices=("core","bot"),nargs="?"); c.add_argument("--path",type=Path,default=Path(".")); c.add_argument("--non-interactive",action="store_true")
    sub.add_parser("info", help="检测项目环境"); sub.add_parser("doctor", help="info 的别名")
    c=sub.add_parser("config", help="配置 Bot config.json"); c.add_argument("--protocol"); c.add_argument("--owner"); c.add_argument("--bot-name"); c.add_argument("--host"); c.add_argument("--port",type=int); c.add_argument("--non-interactive",action="store_true")
    c=sub.add_parser("run", help="使用项目虚拟环境启动 Bot"); c.add_argument("--dry-run",action="store_true"); c.add_argument("args",nargs=argparse.REMAINDER)
    c=sub.add_parser("update-conf", help="配置 Bot 更新渠道"); c.add_argument("--channel",choices=tuple(CHANNELS)); c.add_argument("--non-interactive",action="store_true")
    c=sub.add_parser("upgrade", help="更新 Bot 项目代码"); c.add_argument("--channel",choices=tuple(CHANNELS)); c.add_argument("--dry-run",action="store_true")
    plugin=sub.add_parser("plugin", help="搜索、安装、升级和删除插件").add_subparsers(dest="plugin_command", metavar="PLUGIN_COMMAND")
    c=plugin.add_parser("search", help="搜索智慧市场插件"); c.add_argument("query"); c.add_argument("--refresh",action="store_true"); c.add_argument("--include-deprecated",action="store_true")
    c=plugin.add_parser("show", help="查看本地或远端插件"); c.add_argument("plugin_id"); c.add_argument("-r","--remote",action="store_true"); c.add_argument("-l","--local",action="store_true"); c.add_argument("--refresh",action="store_true")
    c=plugin.add_parser("install", help="下载并安装插件及 PyPI 依赖"); c.add_argument("plugin_id",nargs="?"); c.add_argument("-U","--upgrade",action="store_true"); c.add_argument("--all",action="store_true"); c.add_argument("--dry-run",action="store_true"); c.add_argument("--refresh",action="store_true")
    c=plugin.add_parser("remove", help="删除本地插件"); c.add_argument("plugin_id"); c.add_argument("--no-deps",action="store_true")
    plugin.add_parser("list", help="列出本地插件"); plugin.add_parser("outdated", help="检查插件更新")
    return p

def main(argv=None):
    args=parser().parse_args(argv)
    try: return dispatch(args)
    except JianerError as e: print(f"✗ {e}",file=sys.stderr); return 1
    except subprocess.CalledProcessError as e: print(f"✗ 命令执行失败，退出码 {e.returncode}",file=sys.stderr); return 1

def dispatch(a):
    if a.command=="create": return create(a)
    if a.command in ("info","doctor"): return info(a)
    if a.command=="config": return config_cmd(a)
    if a.command=="run": return run_cmd(a)
    if a.command=="update-conf": return update_conf(a)
    if a.command=="upgrade": return upgrade_cmd(a)
    if a.command=="plugin": return plugin_cmd(a)
    parser().print_help(); return 0

def create(a):
    root=a.path.resolve(); root.mkdir(parents=True,exist_ok=True)
    kind=a.kind or ("bot" if a.non_interactive else input("项目类型 [bot/core] (bot): ").strip() or "bot")
    if kind=="core":
        (root/"plugins").mkdir(exist_ok=True); (root/"main.py").write_text("from jianer import Client\nwith Client() as client:\n    client.run()\n",encoding="utf-8")
    else:
        (root/"plugins").mkdir(exist_ok=True)
        if not (root/"config.json").exists(): (root/"config.json").write_text(json.dumps({"protocol":"OneBot","owner":[],"black_list":[],"silents":[],"connections":{"OneBot":{"mode":"FWS","host":"127.0.0.1","port":5004,"listener_host":"127.0.0.1","listener_port":8081,"retries":5,"token":None,"auth":None}},"log_level":"INFO","log_use_nf":False,"uin":0,"max_workers":25,"others":{}},ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        (root/"main.py").write_text("from cfgr.manager import Serializers\nfrom jianer import Client, configurator\nconfigurator.BotConfig.load_from('config.json', Serializers.JSON, 'jianer-bot')\nfrom jianer.adapters import builtins as adapters\nadapters.load_configured()\nwith Client() as client:\n    result=client.load_plugins('plugins')\n    if result.failed: raise RuntimeError(result.failed)\n    client.run()\n",encoding="utf-8")
    (root/"requirements.txt").write_text("jianer-bot\n",encoding="utf-8"); print(f"✓ 已创建项目：{root}")

def info(a):
    try: p=Project.detect()
    except ProjectError as e:
        if a.json: return emit({"ok":False,"error":str(e)},True)
        raise
    data={"ok":True,"root":str(p.root),"kind":p.kind,"venv":str(p.venv_dir) if p.venv_dir else None}
    if p.kind=="bot": data["plugins"]=len(scan_plugins(p.plugins_dir)); data["config"]=str(p.config_path); data["python"]=p.python
    if emit(data,a.json): return
    print(json.dumps(data,ensure_ascii=False,indent=2))

def config_cmd(a):
    p=Project.bot(); data=read_json(p.config_path); protocol=a.protocol or data.get("protocol","OneBot")
    if a.protocol: data["protocol"]=a.protocol
    if a.owner is not None: data["owner"]=[x for x in a.owner.replace("，",",").replace(" ",",").split(",") if x]
    if a.bot_name is not None: data.setdefault("others",{})["bot_name"]=a.bot_name
    conn=data.setdefault("connections",{}); current=next((v for k,v in conn.items() if k.lower()==protocol.lower()),conn.setdefault(protocol,{}))
    if a.host: current["host"]=a.host
    if a.port: current["port"]=a.port
    if not a.non_interactive and not any(x is not None for x in (a.protocol,a.owner,a.bot_name,a.host,a.port)):
        data["protocol"]=input(f"协议 ({protocol}): ").strip() or protocol
    save_json(p.config_path,data); print(f"✓ 已保存 {p.config_path}（原文件已备份为 .bak）")

def run_cmd(a):
    p=Project.bot(); command=[p.python,str(p.entry_script),*a.args]
    if a.dry_run: print(" ".join(command)); return
    return subprocess.run(command,cwd=p.root).returncode

def update_conf(a):
    p=Project.bot(); channel=a.channel or "main"
    configure(p,channel); print(f"✓ 已配置更新渠道：{CHANNELS[channel]}")

def upgrade_cmd(a):
    p=Project.bot(); data=read_json(p.config_path); channel=a.channel or data.get("jianer_cli_update_channel")
    if not channel: raise ProjectError("尚未配置更新渠道，请先运行 update-conf")
    if a.dry_run: print(f"git fetch origin {channel} && git checkout {channel} && git pull --ff-only origin {channel}"); return
    upgrade(p,channel); print(f"✓ 已更新到 {channel}")

def plugin_cmd(a):
    p=Project.bot() if a.plugin_command not in ("search",) else None
    user=load_user(); market=Market(a.market_url or user.get("market_url"))
    if a.plugin_command=="list":
        items=scan_plugins(p.plugins_dir); values=[x.json() for x in items]
        if emit(values,a.json): return
        table(["插件 ID","版本","类型","状态"],[[x.id or "<invalid>",x.version or "?",x.kind,"禁用" if x.disabled else "启用"] for x in items]); return
    index,plugins=market.index(getattr(a,"refresh",False))
    if a.plugin_command=="search":
        rows=[p for p in plugins if p.matches(a.query) and (a.include_deprecated or not p.deprecated)]; values=[p.__dict__ for p in rows]
        if emit(values,a.json): return
        table(["ID","版本","描述"],[[p.id,p.version,p.description] for p in rows]); return
    if a.plugin_command=="show": return show_plugin(a,p,market,index,plugins)
    if a.plugin_command=="install":
        targets=plugins if a.all else [find_plugin(plugins,a.plugin_id or "")]
        if a.all:
            for target in targets:
                install(market,index,target,p,upgrade=True,all_plugins=False,dry_run=a.dry_run)
            return
        for target in targets: install(market,index,target,p,upgrade=a.upgrade,dry_run=a.dry_run)
        return
    if a.plugin_command=="remove":
        candidates=match_local(scan_plugins(p.plugins_dir),a.plugin_id)
        if not candidates: raise ProjectError(f"本地未安装插件：{a.plugin_id}")
        target=candidates[0]; shutil.rmtree(target.path) if target.path.is_dir() else target.path.unlink(); print(f"✓ 已删除 {target.id or target.path.name}"); return
    if a.plugin_command=="outdated":
        remote={x.id:x for x in plugins}; rows=[]
        for local in scan_plugins(p.plugins_dir):
            if local.id in remote and local.version and tuple(map(int,local.version.split(".")[:3])) < tuple(map(int,remote[local.id].version.split(".")[:3])): rows.append([local.id,local.version,remote[local.id].version])
        table(["插件 ID","本地","市场"],rows)

def show_plugin(a,project,market,index,plugins):
    if not a.remote:
        local=match_local(scan_plugins(project.plugins_dir),a.plugin_id)
        if local and not a.remote:
            x=local[0]; value=x.json()
            if emit(value,a.json): return
            print(json.dumps(value,ensure_ascii=False,indent=2)); return
    remote=find_plugin(plugins,a.plugin_id); value=remote.__dict__
    if emit(value,a.json): return
    print(json.dumps(value,ensure_ascii=False,indent=2))

if __name__ == "__main__": raise SystemExit(main())
