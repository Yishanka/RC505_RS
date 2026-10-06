import unittest
from release_notes import render
from curate_releases import plan, RETIRE, PROTECTED, NEW_TAG


class ReleaseTools(unittest.TestCase):
    def test_page_contains_one_version_and_working_document_links(self):
        history = "# 0.4.8：Install\n\n- Fixed. [Guide](INSTALL_UPDATE_CN.md)\n\n# 0.4.7：Earlier\n\n- Old.\n"
        page = render("0.4.8", history)
        self.assertIn("RC505-RS-0.4.8-windows-x64-setup.exe", page)
        self.assertIn("/blob/v0.4.8/docs/INSTALL_UPDATE_CN.md", page)
        self.assertNotIn("Earlier", page)
        self.assertNotIn("- Old.", page)
        archived = render("0.4.7", history, True)
        self.assertIn("- Old.", archived)
        self.assertNotIn("RC505-RS-0.4.8-windows-x64-setup.exe", archived)
        with self.assertRaises(ValueError):
            render("0.4.9", history)
        with self.assertRaises(ValueError):
            render("../0.4.8", history)

    def releases(self):
        return [{"tagName": tag, "isLatest": tag == NEW_TAG, "isDraft": False, "isPrerelease": False}
                for tag in PROTECTED | RETIRE | {"v0.5.0-preview"}]

    def test_cleanup_is_explicit_and_keeps_baselines_and_unknown_versions(self):
        self.assertEqual(set(plan(self.releases())), RETIRE)
        self.assertFalse(RETIRE & PROTECTED)
        self.assertEqual(plan([r for r in self.releases() if r["tagName"] not in RETIRE]), [])

    def test_cleanup_refuses_missing_baseline_or_unpublished_replacement(self):
        for missing in ["v0.1.1-alpha", NEW_TAG]:
            with self.assertRaises(ValueError):
                plan([r for r in self.releases() if r["tagName"] != missing])
        for flag in ["isDraft", "isPrerelease"]:
            releases = self.releases()
            next(r for r in releases if r["tagName"] == NEW_TAG)[flag] = True
            with self.assertRaises(ValueError):
                plan(releases)
        releases = self.releases()
        for r in releases:
            r["isLatest"] = r["tagName"] == "v0.5.0-preview"
        with self.assertRaises(ValueError):
            plan(releases)


if __name__ == "__main__":
    unittest.main()
