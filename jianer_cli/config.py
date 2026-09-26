from __future__ import annotations
import json, os
from pathlib import Path
from .project import read_json, save_json

def user_path(): return Path(os.getenv("XDG_CONFIG_HOME",Path.home()/".config"))/"jianer-cli/config.json"
def load_user():
    try: return json.loads(user_path().read_text(encoding="utf-8"))
    except (OSError,json.JSONDecodeError): return {"market_url":None,"auto_pip":True}
def save_user(data):
    user_path().parent.mkdir(parents=True,exist_ok=True); user_path().write_text(json.dumps(data,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
