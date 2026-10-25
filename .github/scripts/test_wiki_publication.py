import unittest
from unittest.mock import patch
import urllib.error

import wiki_publication as publication


class PublicationTests(unittest.TestCase):
    def test_missing_or_old_metadata_requires_publish_even_without_new_laws(self):
        self.assertTrue(publication.needs_update({}, "abc"))
        self.assertTrue(publication.needs_update({"source_commit": "old"}, "abc"))
        self.assertFalse(publication.needs_update({"source_commit": "abc"}, "abc"))

    def test_verification_checks_identity_not_only_case_count(self):
        old = {"source_commit": "abc", "pages": [{"path": "hanrei/1", "type": "hanrei"}]}
        new = {"source_commit": "abc", "pages": [{"path": "hanrei/2", "type": "hanrei"}]}
        with self.assertRaises(ValueError):
            publication.verify(old, "abc", publication.case_digest(new))
        with self.assertRaises(ValueError):
            publication.verify(new, "new-sha", publication.case_digest(new))
        publication.verify(new, "abc", publication.case_digest(new))

    def test_transient_server_failure_is_not_treated_as_missing_wiki(self):
        with patch("urllib.request.urlopen", side_effect=urllib.error.HTTPError("url", 503, "Unavailable", {}, None)):
            with self.assertRaises(urllib.error.HTTPError):
                publication.read_public("https://example.com")

    def test_publication_lag_survives_daily_wiki_commits_and_clears_after_publish(self):
        pending, late = publication.publication_watch({}, "one", {}, 100)
        self.assertFalse(late)
        pending, late = publication.publication_watch({}, "two", pending, 100 + 86400)
        self.assertTrue(late)
        self.assertEqual(pending["unpublished_since"], 100)
        state, late = publication.publication_watch({"source_commit": "two"}, "two", pending, 100000)
        self.assertEqual(state, {})
        self.assertFalse(late)

    def test_digest_is_order_independent_and_excludes_other_pages(self):
        cases = [{"path": "hanrei/2", "type": "hanrei"}, {"path": "hanrei/1", "type": "hanrei"}]
        self.assertEqual(publication.case_digest({"pages": cases}), publication.case_digest({"pages": list(reversed(cases)) + [{"path": "laws/1", "type": "law"}]}))


if __name__ == "__main__":
    unittest.main()
