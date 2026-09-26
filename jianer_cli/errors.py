class JianerError(Exception):
    """用户可理解的 CLI 错误。"""

class ProjectError(JianerError): pass
class MarketError(JianerError): pass
class PluginError(JianerError): pass
