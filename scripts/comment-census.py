#!/usr/bin/env python3
"""Count comments in the codebase: per file and per area.

Analyzes comment lines, code lines, ratio, block-size distribution, and blocks
citing issues or ADRs. Designed for tracking comment compaction progress.

    python3 scripts/comment-census.py .
    python3 scripts/comment-census.py crates/core/src
"""

import argparse
import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple


@dataclass
class CommentBlock:
    lines: int
    cites_issue: bool
    cites_adr: bool


@dataclass
class FileStats:
    path: str
    code_lines: int
    comment_lines: int
    blocks: List[CommentBlock]

    @property
    def ratio(self) -> float:
        if self.code_lines == 0:
            return 0.0
        return self.comment_lines / self.code_lines

    @property
    def blocks_over_3(self) -> int:
        return sum(1 for b in self.blocks if b.lines > 3)

    @property
    def blocks_citing_issue(self) -> int:
        return sum(1 for b in self.blocks if b.cites_issue)

    @property
    def blocks_citing_adr(self) -> int:
        return sum(1 for b in self.blocks if b.cites_adr)


ISSUE_PATTERN = re.compile(r'#\d+')
ADR_PATTERN = re.compile(r'ADR-\d{4}')


@dataclass(frozen=True)
class CommentSyntax:
    prefixes: Tuple[str, ...]
    block_open: Optional[str] = None
    block_close: Optional[str] = None
    # HTML markers can sit anywhere on the line, not only at the start.
    infix: bool = False


# One table so a language cannot be counted in is_comment_line and skipped by
# the directory glob. #811.
_C_STYLE = CommentSyntax(("//", "/*", "*"), "/*", "*/")
_HASH = CommentSyntax(("#",))
_POWERSHELL = CommentSyntax(("#", "<#"), "<#", "#>")
_HTML = CommentSyntax(("<!--", "-->"), infix=True)

SYNTAX_BY_EXT: Dict[str, CommentSyntax] = {
    ".rs": _C_STYLE,
    ".js": _C_STYLE,
    ".ts": _C_STYLE,
    ".cjs": _C_STYLE,
    ".css": _C_STYLE,
    ".c": _C_STYLE,
    ".cpp": _C_STYLE,
    ".h": _C_STYLE,
    ".swift": _C_STYLE,
    ".py": _HASH,
    ".sh": _HASH,
    ".yaml": _HASH,
    ".yml": _HASH,
    ".toml": _HASH,
    ".ps1": _POWERSHELL,
    ".html": _HTML,
}


def is_comment_line(line: str, ext: str) -> bool:
    """Check if a line is a comment (handling various language syntaxes)."""
    syntax = SYNTAX_BY_EXT.get(ext)
    if syntax is None:
        return False
    stripped = line.lstrip()
    if syntax.infix:
        return any(marker in stripped for marker in syntax.prefixes)
    return any(stripped.startswith(prefix) for prefix in syntax.prefixes)


def extract_blocks(lines: List[str], ext: str) -> Tuple[int, int, List[CommentBlock]]:
    """Extract comment blocks and count code/comment lines."""
    syntax = SYNTAX_BY_EXT.get(ext)
    code_lines = 0
    comment_lines = 0
    blocks = []

    current_block_lines = []
    in_multiline = False

    for line in lines:
        stripped = line.strip()

        if not stripped:
            if in_multiline:
                current_block_lines.append(line)
                comment_lines += 1
                continue
            if current_block_lines:
                blocks.append(analyze_block(current_block_lines))
                current_block_lines = []
            continue

        is_comment = is_comment_line(line, ext)

        if syntax is not None and syntax.block_open and syntax.block_close:
            if syntax.block_open in stripped:
                in_multiline = True
            if syntax.block_close in stripped:
                is_comment = True
                current_block_lines.append(line)
                comment_lines += 1
                in_multiline = False
                continue

        if is_comment or in_multiline:
            current_block_lines.append(line)
            comment_lines += 1
        else:
            if current_block_lines:
                blocks.append(analyze_block(current_block_lines))
                current_block_lines = []
                in_multiline = False
            code_lines += 1

    if current_block_lines:
        blocks.append(analyze_block(current_block_lines))

    return code_lines, comment_lines, blocks


def analyze_block(lines: List[str]) -> CommentBlock:
    """Analyze a comment block for citations."""
    text = ' '.join(lines)
    text_without_entities = re.sub(r'&#\d+;', '', text)
    return CommentBlock(
        lines=len(lines),
        cites_issue=bool(ISSUE_PATTERN.search(text_without_entities)),
        cites_adr=bool(ADR_PATTERN.search(text_without_entities))
    )


def scan_file(path: Path) -> FileStats:
    """Scan a single file for comment statistics."""
    try:
        with open(path, 'r', encoding='utf-8') as f:
            lines = f.readlines()
    except (UnicodeDecodeError, PermissionError):
        return FileStats(str(path), 0, 0, [])

    ext = path.suffix
    code_lines, comment_lines, blocks = extract_blocks(lines, ext)

    return FileStats(
        path=str(path),
        code_lines=code_lines,
        comment_lines=comment_lines,
        blocks=blocks
    )


def scan_directory(root: Path) -> List[FileStats]:
    """Recursively scan a directory for source files."""
    stats = []
    for ext in SYNTAX_BY_EXT:
        for path in root.rglob(f"*{ext}"):
            if any(part.startswith('.') for part in path.parts):
                continue
            if 'target' in path.parts or 'node_modules' in path.parts:
                continue

            file_stats = scan_file(path)
            if file_stats.code_lines > 0 or file_stats.comment_lines > 0:
                stats.append(file_stats)

    return stats


def print_summary(stats: List[FileStats], label: str = "Total"):
    """Print summary statistics."""
    if not stats:
        print(f"{label}: No files found")
        return

    total_code = sum(s.code_lines for s in stats)
    total_comments = sum(s.comment_lines for s in stats)
    total_blocks = sum(len(s.blocks) for s in stats)
    blocks_over_3 = sum(s.blocks_over_3 for s in stats)
    blocks_citing_issue = sum(s.blocks_citing_issue for s in stats)
    blocks_citing_adr = sum(s.blocks_citing_adr for s in stats)

    ratio = (total_comments / total_code * 100) if total_code > 0 else 0

    print(f"\n{label}:")
    print(f"  Files: {len(stats)}")
    print(f"  Code lines: {total_code:,}")
    print(f"  Comment lines: {total_comments:,}")
    print(f"  Ratio: {ratio:.1f}%")
    print(f"  Blocks: {total_blocks:,}")
    print(f"  Blocks >3 lines: {blocks_over_3:,} ({blocks_over_3/total_blocks*100:.1f}%)" if total_blocks > 0 else "  Blocks >3 lines: 0")
    print(f"  Blocks citing issue: {blocks_citing_issue:,}")
    print(f"  Blocks citing ADR: {blocks_citing_adr:,}")

    if stats and len(stats) <= 20:
        print("\n  Per file:")
        for s in sorted(stats, key=lambda x: x.comment_lines, reverse=True):
            print(f"    {Path(s.path).name}: {s.comment_lines} comments, {s.code_lines} code, {s.ratio*100:.1f}% ratio")


def main():
    parser = argparse.ArgumentParser(description="Count comments in source code")
    parser.add_argument("path", help="File or directory to analyze")
    parser.add_argument("--by-area", action="store_true", help="Break down by area")

    args = parser.parse_args()

    path = Path(args.path)

    if not path.exists():
        print(f"Error: {path} does not exist", file=sys.stderr)
        return 1

    if path.is_file():
        stats = [scan_file(path)]
        print_summary(stats, str(path))
    else:
        stats = scan_directory(path)
        print_summary(stats, str(path))

        if args.by_area:
            areas = defaultdict(list)
            for s in stats:
                p = Path(s.path)
                if 'crates/core' in str(p):
                    areas['Engine (crates/core)'].append(s)
                elif 'src-tauri/src/model' in str(p) or 'src-tauri/src/harness' in str(p) or 'src-tauri/src/acp_wire' in str(p):
                    areas['AI lane (model/harness/acp_wire)'].append(s)
                elif 'src-tauri/src' in str(p):
                    areas['Shell & platform (src-tauri/src)'].append(s)
                elif str(p).startswith('src/') or str(p).startswith('scripts/') or str(p).startswith('tests/'):
                    areas['Webview, scripts & tests'].append(s)

            for area_name, area_stats in sorted(areas.items()):
                print_summary(area_stats, area_name)

    return 0


if __name__ == '__main__':
    sys.exit(main())
