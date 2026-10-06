"""Create a short, version-specific GitHub release page."""
from pathlib import Path
import argparse
import re

REPO = "https://github.com/Yishanka/RC505_RS"


def render(version: str, history: str, archived: bool = False) -> str:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Expected a numeric release version")
    heading = re.search(rf"(?m)^# {re.escape(version)}(?=[:： \n]).*$", history)
    if heading is None:
        raise ValueError(f"Missing release notes for {version}")
    remainder = history[heading.end():]
    end = re.search(r"(?m)^# ", remainder)
    changes = remainder[:end.start()] if end else remainder
    changes = re.sub(
        r"\]\((?!https?://|#)([^)]+)\)",
        lambda m: f"]({REPO}/blob/v{version}/docs/{m[1]})",
        changes.strip(),
    )
    title = heading[0].removeprefix("# ")
    if archived:
        return f"历史版本 · [下载最新版]({REPO}/releases/latest)\n\n## {title}\n\n{changes}\n"
    return f"""## 下载与安装 / Download and install

仅支持 Windows x64。下拉展开 **Assets**，下载 **`RC505-RS-{version}-windows-x64-setup.exe`**。

1. 只下载这一个安装包即可；不需要 Source code、Rust 或其他依赖。
2. 下载完成后双击，按向导选择程序、数据和安装包下载目录。
3. 新安装或普通更新可跳过导入。点 Install，完成后勾选 Open RC505 RS，再点 Finish。
4. 出现工程选择页即完成安装。已安装旧版时，先保存并退出，无需先卸载。

[完整安装步骤与下载提示]({REPO}/blob/main/docs/INSTALL_UPDATE_CN.md) · [English instructions]({REPO}/blob/main/README.md#download-install-and-open)

**便携版 / Portable:** `RC505-RS-{version}-windows-x64-portable.zip`，完整解压后运行 `rc505_rs.exe`。
`SHA256SUMS.txt` 供核对下载，`update.json` 供软件更新使用；**Source code** 是源码，不是安装包。

## {title}

{changes}
"""


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("output", type=Path)
    parser.add_argument("--archived", action="store_true")
    args = parser.parse_args()
    workspace = Path(__file__).resolve().parent.parent
    notes = render(args.version, (workspace / "docs/RELEASE_NOTES_CN.md").read_text(encoding="utf-8-sig"), args.archived)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(notes, encoding="utf-8")
