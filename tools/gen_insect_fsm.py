# Generates PROGRAM_FSM for rust/src/insect_actor.rs from ac-decomp src/actor/ac_ins_*.c.
# Usage: python -I tools/gen_insect_fsm.py <decomp-root> [--json]
"""Extract each insect behaviour program's action list and transition graph.

For every ac_ins_*.c: the action enum (aIXX_ACTION_*), the act_proc[] and
init_proc[] tables used by the setupAction function, and for every action the
set of actions it can switch to (setupAction calls inside its act proc, inside
the init proc, and inside the helpers they call one level deep), plus the
transitions forced from the actor move function.
Emits JSON."""
import glob, json, os, re, sys

d = sys.argv[1]
out = {}

# Map aINS_INSECT_TYPE_* ordinals: retail sometimes passes a TYPE constant
# where an ACTION is expected (e.g. chou's pansy rest passes PURPLE_BUTTERFLY=3
# = ACTION_LANDING). Parse the header enum so those calls resolve by value.
_type_ord = {}
try:
    hdr = open(os.path.join(d, "include/ac_insect_h.h"), encoding="utf-8").read()
    em = re.search(r"enum insect_type\s*\{([^}]*)\}", hdr, re.S)
    if em:
        _type_ord = {n.strip(): i for i, n in enumerate(
            [x.split("=")[0].strip() for x in em.group(1).split(",") if x.strip()])}
except OSError:
    pass


def strip_comments(s):
    s = re.sub(r"/\*.*?\*/", "", s, flags=re.S)
    return re.sub(r"//[^\n]*", "", s)


def functions(src):
    fns = {}
    for m in re.finditer(r"\n(?:static |extern )?[\w\s\*]+?\b(\w+)\(([^;{]*?)\)\s*\{", src):
        name = m.group(1)
        start = m.end()
        depth, i = 1, start
        while depth and i < len(src):
            if src[i] == "{":
                depth += 1
            elif src[i] == "}":
                depth -= 1
            i += 1
        fns[name] = src[start:i - 1]
    return fns


for f in sorted(glob.glob(os.path.join(d, "src/actor/ac_ins_*.c"))):
    src = strip_comments(open(f, encoding="utf-8").read())
    fam = os.path.basename(f)[7:-2]
    enum = re.search(r"enum\s*\{([^}]*?_ACT(?:ION)?_[^}]*?)\}", src, re.S)
    if not enum:
        continue
    actions = [a.strip().split("=")[0].strip() for a in enum.group(1).split(",") if a.strip()]
    sep = "_ACTION_" if any("_ACTION_" in a for a in actions) else "_ACT_"
    actions = [a for a in actions if sep in a and not a.endswith("_NUM")]
    prefix = actions[0].split(sep)[0]
    fns = functions(src)
    setup_name = next((n for n in fns if n.startswith(prefix) and n.endswith("setupAction")), None)
    setup = fns.get(setup_name, "")
    tabs = re.findall(r"(\w+)\[[^\]]*\]\s*=\s*\{(.*?)\};", setup, re.S)
    procs = {}
    for tname, body in tabs:
        procs[tname] = [x.strip().lstrip("&").replace("(aINS_ACTION_PROC)", "").strip() for x in body.split(",") if x.strip()]
    act_tab = next((v for k, v in procs.items() if "act" in k.lower() or "process" in k.lower()), [])
    init_tab = next((v for k, v in procs.items() if "init" in k.lower()), [])
    call_re = re.compile(re.escape(setup_name) + r"\s*\(\s*\w+\s*,\s*(" + re.escape(prefix + sep) + r"\w+)") if setup_name else None

    type_call_re = re.compile(re.escape(setup_name) + r"\s*\(\s*\w+\s*,\s*(aINS_INSECT_TYPE_\w+)") if setup_name else None

    def targets(fn_name, depth=0):
        body = fns.get(fn_name, "")
        t = set(call_re.findall(body)) if call_re else set()
        if type_call_re and _type_ord:
            for tc in type_call_re.findall(body):
                if tc in _type_ord and _type_ord[tc] < len(actions):
                    t.add(actions[_type_ord[tc]])
        if setup_name:
            for var in re.findall(re.escape(setup_name) + r"\s*\(\s*\w+\s*,\s*([a-z]\w*)\s*,", body):
                t |= set(re.findall(r"\b" + var + r"\s*=\s*(" + re.escape(prefix + sep) + r"\w+)", body))
        if depth < 1:
            for callee in re.findall(r"\b(" + re.escape(prefix) + r"_\w+)\s*\(", body):
                if callee != setup_name and callee in fns and callee != fn_name:
                    t |= targets(callee, depth + 1)
        return t

    fsm = {}
    for i, a in enumerate(actions):
        ap = act_tab[i] if i < len(act_tab) else None
        ip = init_tab[i] if i < len(init_tab) else None
        t = set()
        if ap:
            t |= targets(ap)
        if ip:
            t |= targets(ip)
        fsm[a.split(sep)[1]] = {
            "act": ap, "init": ip,
            "to": sorted(x.split(sep)[1] for x in t),
        }
    move = next((n for n in fns if n.startswith(prefix) and n.endswith("actor_move")), None)
    forced = sorted(set(x.split(sep)[1] for x in (targets(move, 1) if move else set())))
    init_fn = next((n for n in fns if n.startswith(prefix) and n.endswith("actor_init")), None)
    initial = sorted(set(x.split(sep)[1] for x in (targets(init_fn, 1) if init_fn else set())))
    ct = next((n for n in fns if n.startswith(prefix) and (n.endswith("_ct") or n.endswith("actor_ct"))), None)
    out[fam] = {"prefix": prefix, "actions": [a.split(sep)[1] for a in actions], "fsm": fsm,
                "forced_from_move": forced, "initial": initial, "setup": setup_name, "move": move}
EMIT = True

ORDER = ["chou", "batta", "tonbo", "tentou", "hotaru", "semi", "kabuto", "goki", "hitodama", "amenbo", "ka", "dango",
         "kera", "mino"]
if "--json" in sys.argv:
    json.dump(out, sys.stdout, indent=1)
    sys.exit(0)
print("/// Behaviour-program state machines (index = `P_*`), extracted from")
print("/// `ac_ins_*.c` by tools/gen_insect_fsm.py: per action, the actions its act")
print("/// proc, init proc and one level of helpers can switch to (constant and")
print("/// computed `setupAction` arguments), the actions `actor_init` can start")
print("/// in, and those `actor_move` forces (net catch / lifetime escape).")
print("pub const PROGRAM_FSM: [ProgramFsm; 14] = [")
for fam in ORDER:
    v = out[fam]
    acts = v["actions"]
    idx = {a: i for i, a in enumerate(acts)}
    names = ", ".join(f'"{a}"' for a in acts)
    trans = ", ".join("&[" + ", ".join(str(idx[t]) for t in v["fsm"][a]["to"]) + "]" for a in acts)
    init = ", ".join(str(idx[a]) for a in v["initial"])
    forced = ", ".join(str(idx[a]) for a in v["forced_from_move"])
    print(f"    ProgramFsm {{ family: \"{fam}\", actions: &[{names}],")
    print(f"        transitions: &[{trans}],")
    print(f"        initial: &[{init}], forced: &[{forced}] }},")
print("];")
