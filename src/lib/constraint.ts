/**
 * Composer-style version constraints ("^8.2", ">=8.1 <8.4", "~8.3.0", "8.2.*", "^8.1 || ^9.0"),
 * enough to tell whether a PHP version meets a project's composer.json `require.php`.
 * Anything it can't read counts as met, so an odd constraint never shows a false warning.
 */

type Version = [number, number, number];

/** "8.5.12-1" → [8, 5, 12]; null for something that isn't a version. */
export function parseVersion(text: string): Version | null {
  const m = /^v?(\d+)(?:\.(\d+))?(?:\.(\d+))?/.exec(text.trim());
  return m ? [Number(m[1]), Number(m[2] ?? 0), Number(m[3] ?? 0)] : null;
}

const cmp = (a: Version, b: Version) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];

/** How many parts a constraint's version names: "8" 1, "8.2" 2, "8.2.1" 3. */
const parts = (text: string) => text.replace(/^v/, "").split(/[.-]/).filter((p) => /^\d+$/.test(p)).length;

/** One condition ("^8.2", ">=8.1", "8.2.*"): met by `v`, or null when it can't be read. */
function one(atom: string, v: Version): boolean | null {
  const a = atom.replace(/@\w+$/, "").trim();
  if (a === "*" || a === "") return true;
  const wildcard = /^v?(\d+)(?:\.(\d+))?\.[*x]$/.exec(a);
  if (wildcard) {
    const major = Number(wildcard[1]);
    return wildcard[2] == null ? v[0] === major : v[0] === major && v[1] === Number(wildcard[2]);
  }
  const m = /^(\^|~|>=|<=|>|<|!=|==|=)?\s*(.+)$/.exec(a);
  if (!m) return null;
  const base = parseVersion(m[2]);
  if (!base) return null;
  const n = parts(m[2]);
  switch (m[1]) {
    case "^": {
      // ^8.2 → >=8.2 <9; ^0.3 → >=0.3 <0.4.
      const upper: Version = base[0] > 0 ? [base[0] + 1, 0, 0] : [0, base[1] + 1, 0];
      return cmp(v, base) >= 0 && cmp(v, upper) < 0;
    }
    case "~": {
      // ~8.2 → >=8.2 <9; ~8.2.1 → >=8.2.1 <8.3.
      const upper: Version = n >= 3 ? [base[0], base[1] + 1, 0] : [base[0] + 1, 0, 0];
      return cmp(v, base) >= 0 && cmp(v, upper) < 0;
    }
    case ">=":
      return cmp(v, base) >= 0;
    case "<=":
      return cmp(v, base) <= 0;
    case ">":
      return cmp(v, base) > 0;
    case "<":
      return cmp(v, base) < 0;
    case "!=":
      return cmp(v, base) !== 0;
    default:
      // "8.2" means that line: 8.2.x.
      return n >= 3 ? cmp(v, base) === 0 : v[0] === base[0] && (n < 2 || v[1] === base[1]);
  }
}

/** Whether `version` meets `constraint`; true when either can't be read. */
export function satisfies(version: string, constraint: string): boolean {
  const v = parseVersion(version);
  if (!v) return true;
  return constraint.split(/\s*\|\|?\s*/).some((alternative) => {
    // Spaces or commas join conditions that all have to hold; "8.1 - 8.3" is a range.
    const range = /^(\S+)\s+-\s+(\S+)$/.exec(alternative.trim());
    const atoms = range
      ? [`>=${range[1]}`, `<=${range[2]}`]
      : alternative
          .replace(/(>=|<=|!=|==|>|<|=|\^|~)\s+/g, "$1")
          .split(/[\s,]+/)
          .filter(Boolean);
    const results = atoms.map((atom) => one(atom, v));
    return results.every((r) => r !== false);
  });
}

/** The newest of `versions` that meets `constraint`, or null. */
export function bestMatch(versions: string[], constraint: string): string | null {
  const ok = versions.filter((v) => satisfies(v, constraint) && parseVersion(v));
  ok.sort((a, b) => cmp(parseVersion(b)!, parseVersion(a)!));
  return ok[0] ?? null;
}
