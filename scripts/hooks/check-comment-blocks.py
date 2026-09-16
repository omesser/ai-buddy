#!/usr/bin/env python3
"""Pre-commit hook: check for newly added comment blocks over 3 lines.

Only fires on changed files and only on newly added comment blocks, never on
the existing tree. This guards against comment bloat without blocking work that
legitimately touches long existing blocks.
"""

import re
import subprocess
import sys
from pathlib import Path


def get_added_lines(filename):
    """Get line numbers of added lines from git diff."""
    try:
        result = subprocess.run(
            ["git", "diff", "--cached", "--unified=0", "--", filename],
            capture_output=True,
            text=True,
            check=True
        )
    except subprocess.CalledProcessError:
        return set()

    added_lines = set()
    current_line = 0

    for line in result.stdout.splitlines():
        if line.startswith('@@'):
            match = re.match(r'@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@', line)
            if match:
                current_line = int(match.group(1))
        elif line.startswith('+') and not line.startswith('+++'):
            added_lines.add(current_line)
            current_line += 1
        elif not line.startswith('-'):
            current_line += 1

    return added_lines


def is_comment_line(line, ext):
    """Check if a line is a comment."""
    stripped = line.lstrip()
    
    if ext in ['.rs', '.js', '.ts', '.css', '.c', '.cpp', '.h']:
        return stripped.startswith('//') or stripped.startswith('/*') or stripped.startswith('*')
    elif ext in ['.py', '.sh', '.yaml', '.yml', '.toml']:
        return stripped.startswith('#')
    elif ext in ['.html']:
        return '<!--' in stripped or '-->' in stripped
    
    return False


def find_new_long_blocks(filename):
    """Find newly added comment blocks over 3 lines in a file."""
    path = Path(filename)
    ext = path.suffix
    
    if ext not in ['.rs', '.js', '.ts', '.py', '.css', '.sh', '.html', '.toml', '.yaml', '.yml']:
        return []
    
    try:
        with open(filename, 'r', encoding='utf-8') as f:
            lines = f.readlines()
    except (UnicodeDecodeError, PermissionError, FileNotFoundError):
        return []
    
    added_lines = get_added_lines(filename)
    if not added_lines:
        return []
    
    violations = []
    current_block_start = None
    current_block_lines = []
    in_multiline = False

    for i, line in enumerate(lines, start=1):
        stripped = line.strip()
        
        if not stripped:
            if current_block_lines and len(current_block_lines) > 3:
                newly_added = sum(1 for ln in current_block_lines if ln in added_lines)
                if newly_added > 0:
                    violations.append((current_block_start, len(current_block_lines)))
            current_block_start = None
            current_block_lines = []
            in_multiline = False
            continue
        
        is_comment = is_comment_line(line, ext)
        
        if ext in ['.rs', '.js', '.ts', '.css', '.c', '.cpp', '.h']:
            if '/*' in stripped:
                in_multiline = True
            if '*/' in stripped:
                is_comment = True
                if current_block_start is None:
                    current_block_start = i
                current_block_lines.append(i)
                in_multiline = False
                continue
        
        if is_comment or in_multiline:
            if current_block_start is None:
                current_block_start = i
            current_block_lines.append(i)
        else:
            if current_block_lines and len(current_block_lines) > 3:
                newly_added = sum(1 for ln in current_block_lines if ln in added_lines)
                if newly_added > 0:
                    violations.append((current_block_start, len(current_block_lines)))
            current_block_start = None
            current_block_lines = []
            in_multiline = False
    
    if current_block_lines and len(current_block_lines) > 3:
        newly_added = sum(1 for ln in current_block_lines if ln in added_lines)
        if newly_added > 0:
            violations.append((current_block_start, len(current_block_lines)))
    
    return violations


def main():
    """Check all staged files for new long comment blocks."""
    if len(sys.argv) < 2:
        print("Usage: check-comment-blocks.py <file>...", file=sys.stderr)
        return 1
    
    violations = []
    for filename in sys.argv[1:]:
        file_violations = find_new_long_blocks(filename)
        if file_violations:
            violations.append((filename, file_violations))
    
    if violations:
        print("Comment blocks over 3 lines detected in newly added code:", file=sys.stderr)
        print(file=sys.stderr)
        for filename, blocks in violations:
            print(f"  {filename}:", file=sys.stderr)
            for start_line, length in blocks:
                print(f"    Line {start_line}: {length}-line block", file=sys.stderr)
        print(file=sys.stderr)
        print("Per docs/agents/comments.md, keep comment blocks at 3 lines or fewer.", file=sys.stderr)
        print("To commit anyway: git commit --no-verify", file=sys.stderr)
        return 1
    
    return 0


if __name__ == '__main__':
    sys.exit(main())
