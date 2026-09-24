"""Build the fixed 50-page Wikipedia corpus used by the local performance baseline."""

from pathlib import Path
import shutil
import sys


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: prepare_wikipedia_fixtures.py OUTPUT_DIRECTORY")

    source = Path(__file__).resolve().parents[1] / "tests/upstream/defuddle/fixtures"
    pages = sorted(source.glob("wikipedia-zim--*.html"))
    if len(pages) != 50:
        raise SystemExit(f"expected 50 Wikipedia fixtures, found {len(pages)}")

    destination = Path(sys.argv[1])
    destination.mkdir(parents=True, exist_ok=True)
    if any(destination.iterdir()):
        raise SystemExit(f"output directory must be empty: {destination}")

    html_dir = destination / "A"
    html_dir.mkdir()
    with (destination / "manifest.tsv").open("w", encoding="utf-8") as manifest:
        for page in pages:
            shutil.copyfile(page, html_dir / page.name)
            title = page.stem.removeprefix("wikipedia-zim--")
            manifest.write(f"A\t{page.name}\t{title}\n")

    print(f"prepared {len(pages)} pages in {destination}")


if __name__ == "__main__":
    main()
