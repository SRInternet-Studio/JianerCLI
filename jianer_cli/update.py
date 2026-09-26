from __future__ import annotations
import subprocess, time
from .errors import ProjectError
CHANNELS={"main":"main (Fixed Release)","NEXT-PREVIEW":"NEXT-PREVIEW (Curated Rolling Release)","dev":"dev (Rolling Release)"}
def configure(project, channel):
    if channel not in CHANNELS: raise ProjectError("渠道只能是 main、NEXT-PREVIEW 或 dev")
    if channel=="dev": print("警告：dev 分支不稳定，不建议用于生产环境。10 秒后继续……"); time.sleep(10)
    data=__import__('jianer_cli.project',fromlist=['read_json']).read_json(project.config_path); data["jianer_cli_update_channel"]=channel; __import__('jianer_cli.project',fromlist=['save_json']).save_json(project.config_path,data); return channel
def upgrade(project,channel):
    for args in (("fetch","origin",channel),("checkout",channel),("pull","--ff-only","origin",channel)):
        subprocess.run(["git",*args],cwd=project.root,check=True)
