import json
def emit(value, as_json=False):
    if as_json: print(json.dumps(value,ensure_ascii=False,indent=2)); return True
    return False
def table(headers,rows):
    print("  ".join(headers)); print("  ".join("-"*len(h) for h in headers))
    for row in rows: print("  ".join(str(x) for x in row))
