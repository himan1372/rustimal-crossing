# Prints the `ACTOR_PROFILE_NAMES` table for rust/src/engine_tables.rs from
# src/game/m_actor_dlftbls.c (index = mAc_PROFILE_* id, None = MAKE_ACTOR_NULL).
# Usage: python -I tools/gen_actor_profiles.py <decomp>/src/game/m_actor_dlftbls.c
import re
import sys

txt = open(sys.argv[1], encoding="utf-8").read().replace("\r", "")
body = re.search(r"ACTOR_DLFTBL actor_dlftbls\[\] = \{(.*?)\n\};", txt, re.S).group(1)
entries = re.findall(r"MAKE_ACTOR_DLF\((\w+)\)|MAKE_ACTOR_NULL\(\)", body)
names = [f'Some("{e}")' if e else "None" for e in entries]
print(f"pub const ACTOR_PROFILE_NAMES: [Option<&str>; {len(names)}] = [")
for i in range(0, len(names), 6):
    print("    " + ", ".join(names[i : i + 6]) + ",")
print("];")
