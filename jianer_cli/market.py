from __future__ import annotations
import hashlib, json, os, re, shutil, tempfile, urllib.request, zipfile
from dataclasses import dataclass
from pathlib import Path
from .errors import MarketError, PluginError
DEFAULT_INDEX = "https://raw.githubusercontent.com/Welsonpeaches/Jianer_Plugins_Index/main/index.json"

@dataclass
class MarketPlugin:
    id: str; name: str; version: str; description: str; usage: str; path: str; entry: str
    requires: list[str]; pip_dependencies: list[str]; plugin_type: str; deprecated: bool
    release: dict; authors: list[str]; homepage: str | None = None; tags: list[str] | None = None
    def matches(self, q):
        q=q.lower(); return any(q in str(x).lower() for x in (self.id,self.name,self.description,*(self.tags or [])))
    @classmethod
    def from_dict(cls, d):
        return cls(d["id"], d.get("name",d["id"]), d["version"], d.get("description",""), d.get("usage",""), d.get("path",""), d.get("entry",""), d.get("requires",[]), d.get("pipDependencies",[]), d.get("type","package"), d.get("deprecated",False), d["release"], d.get("authors",[]), d.get("homepage"), d.get("tags",[]))

class Market:
    def __init__(self, url=None):
        self.url = url or os.getenv("JIANER_MARKET_URL") or DEFAULT_INDEX
        cache = Path(os.getenv("XDG_CACHE_HOME", Path.home()/".cache")) / "jianer-cli" / "index.json"
        self.cache = cache
    def index(self, refresh=False):
        if not refresh and self.cache.is_file():
            import time
            if time.time()-self.cache.stat().st_mtime < 600:
                try: return self._parse(self.cache.read_text(encoding="utf-8"))
                except MarketError: pass
        try:
            request=urllib.request.Request(self.url, headers={"User-Agent":"jianer-cli/0.2"})
            with urllib.request.urlopen(request, timeout=60) as response: text=response.read().decode("utf-8")
        except Exception as e: raise MarketError(f"请求市场索引失败：{e}") from e
        index=self._parse(text); self.cache.parent.mkdir(parents=True, exist_ok=True); self.cache.write_text(text,encoding="utf-8"); return index
    def _parse(self,text):
        try: data=json.loads(text)
        except json.JSONDecodeError as e: raise MarketError(f"市场索引 JSON 无效：{e}") from e
        if isinstance(data,dict) and "plugins" in data:
            repo=data.get("repository",{}); return data, [MarketPlugin.from_dict(p) for p in data["plugins"]]
        # 兼容现有 Jianer_Plugins_Index：它没有 index.json，由 CLI 仅支持手工 URL/未来转换。
        raise MarketError("当前市场地址不是 JianerCLI schema v1 index.json；旧版插件市场需要先生成兼容索引")
    def asset_url(self,index,plugin):
        repo=index.get("repository",{}); template=repo.get("releaseUrlTemplate")
        if template: return template.replace("{tag}",plugin.release["tag"]).replace("{asset}",plugin.release["asset"])
        return f"https://github.com/{repo.get('url','').rstrip('/').split('github.com/')[-1]}/releases/download/{plugin.release['tag']}/{plugin.release['asset']}"
    def download(self,index,plugin):
        url=self.asset_url(index,plugin); target=self.cache.parent/"downloads"/Path(plugin.release["asset"]).name; target.parent.mkdir(parents=True,exist_ok=True)
        try:
            with urllib.request.urlopen(urllib.request.Request(url,headers={"User-Agent":"jianer-cli/0.2"}),timeout=120) as r: data=r.read()
        except Exception as e: raise MarketError(f"下载 {plugin.id} 失败：{e}") from e
        actual=hashlib.sha256(data).hexdigest(); expected=plugin.release.get("sha256","")
        if expected and actual.lower()!=expected.lower(): raise MarketError(f"{plugin.id} SHA-256 校验失败：{actual} != {expected}")
        target.write_bytes(data); return target

def find_plugin(plugins, query):
    q=query.lower(); exact=[p for p in plugins if p.id.lower()==q or p.id.removeprefix("jianerbot-plugin-").lower()==q]
    candidates=exact or [p for p in plugins if p.matches(query)]
    if not candidates: raise PluginError(f"智慧市场中找不到插件：{query}")
    if len(candidates)>1 and not exact: raise PluginError(f"'{query}' 匹配到多个插件：{', '.join(p.id for p in candidates)}")
    return candidates[0]
