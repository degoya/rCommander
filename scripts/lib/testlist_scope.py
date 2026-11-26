"""Which jobs a test list holds, for scripts/lib/testlist.py: the delta of a release.

A job is in the delta since a tag (`--since <tag>`) when a commit after the tag names its id
(`git log <tag>..HEAD`, subject or body), or when its job file's status differs from the one it
had at the tag - a file the tag does not have counts as changed. `--before <tag>` is the rest:
the backlog the owner ticks once with the bulk smoke list. `latest` is the newest `v*` tag.
"""

import re
import subprocess

from archive_jobs_status import JOBS, STATUS_PREFIX
from job_boxes import job_id

JOB_REF = re.compile(r"RC-\d{3}-\d{2}")


def full_status(text):
    """The status after its prefix, with the lines it wraps onto joined; None without one."""
    lines = text.split("\n")
    at = next((i for i, l in enumerate(lines) if l.strip().startswith(STATUS_PREFIX)), None)
    if at is None:
        return None
    parts = [lines[at].strip()[len(STATUS_PREFIX):].strip()]
    for line in lines[at + 1:]:
        if not line.startswith(" ") or not line.strip():
            break
        parts.append(line.strip())
    return " ".join(parts)


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True)


def resolve_tag(repo, tag):
    """The tag's name (`latest`: the newest `v*` tag by date), or None when it is no commit."""
    if tag == "latest":
        tags = git(repo, "tag", "-l", "v*", "--sort=-creatordate").stdout.split()
        if not tags:
            return None
        tag = tags[0]
    if git(repo, "rev-parse", "--verify", "--quiet", f"{tag}^{{commit}}").returncode != 0:
        return None
    return tag


def changed_since(repo, tag):
    """The ids of the jobs with a commit after `tag` or a status other than at `tag`."""
    found = set(JOB_REF.findall(git(repo, "log", "--format=%B", f"{tag}..HEAD").stdout))
    changed = git(repo, "diff", "--name-only", tag, "--", f"{JOBS}/").stdout.split("\n")
    for path in changed:
        name = path.rsplit("/", 1)[-1]
        job = job_id(name)
        if not path.startswith(f"{JOBS}/") or job is None or job in found:
            continue
        try:
            now = open(f"{repo}/{path}", encoding="utf-8").read()
        except OSError:
            continue
        then = git(repo, "show", f"{tag}:{path}")
        if then.returncode != 0 or full_status(then.stdout) != full_status(now):
            found.add(job)
    return found
