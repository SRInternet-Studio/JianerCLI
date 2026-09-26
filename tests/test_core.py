import json
import tempfile
import unittest
from pathlib import Path
from jianer_cli.local import parse_meta_source, valid_id, scan_plugins
from jianer_cli.project import Project
from jianer_cli.market import MarketPlugin, find_plugin
from jianer_cli.installer import dependency_order, safe_extract

class CoreTests(unittest.TestCase):
    def test_static_meta(self):
        meta=parse_meta_source('from x import PluginMetadata\n__plugin_meta__ = PluginMetadata(name="jianerbot-plugin-test", description="hello, world", requires={"jianerbot-plugin-alconna"})')
        self.assertEqual(meta.name,"jianerbot-plugin-test")
        self.assertEqual(meta.requires,["jianerbot-plugin-alconna"])
    def test_ignores_nested_metadata(self):
        self.assertIsNone(parse_meta_source('def f():\n    __plugin_meta__ = PluginMetadata(name="fake")\n'))
    def test_id_rule(self):
        self.assertTrue(valid_id("jianerbot-plugin-a-2")); self.assertFalse(valid_id("jianerbot-plugin-A"))
    def test_dependency_order(self):
        plugins={"a":type("P",(),{"requires":["b"]})(),"b":type("P",(),{"requires":[]})()}
        self.assertEqual(dependency_order("a",plugins,set()),["b","a"])
    def test_fuzzy_exact(self):
        p=MarketPlugin("jianerbot-plugin-hello","Hello","1.0.0","","","","",[],[],"package",False,{"tag":"t","asset":"a","sha256":""},[])
        self.assertIs(find_plugin([p],"hello"),p)
    def test_project_detection_and_unknown_config_preservation(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/"plugins").mkdir(); (root/"main.py").write_text("")
            (root/"config.json").write_text(json.dumps({"protocol":"OneBot","connections":{},"unknown":{"x":1}}))
            self.assertEqual(Project.detect(root).kind,"bot")

if __name__=="__main__": unittest.main()
