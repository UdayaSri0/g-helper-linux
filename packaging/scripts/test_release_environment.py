import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("release_environment", Path(__file__).with_name("check-release-environment.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


def configured():
    return ({"name": "release-publish", "can_admins_bypass": False,
             "protection_rules": [{"type": "required_reviewers", "prevent_self_review": True,
                                   "reviewers": [{"type": "User", "reviewer": {"login": "independent"}}]}],
             "deployment_branch_policy": {"protected_branches": False, "custom_branch_policies": True}},
            {"total_count": 1, "branch_policies": [{"type": "tag", "name": "v0.4.1"}]})


class ReleaseEnvironmentTests(unittest.TestCase):
    def test_accepts_saved_independent_approval_gate(self):
        env, policies = configured()
        self.assertEqual(release.validate(env, policies, "publisher"), [])

    def test_rejects_missing_bypass_self_approval_and_wrong_tag(self):
        env, policies = configured()
        env["can_admins_bypass"] = True
        env["protection_rules"][0]["prevent_self_review"] = False
        env["protection_rules"][0]["reviewers"] = [{"type": "User", "reviewer": {"login": "publisher"}}]
        policies["branch_policies"][0]["name"] = "v*"
        errors = release.validate(env, policies, "publisher")
        self.assertEqual(len(errors), 4)

    def test_rejects_missing_or_unprotected_environment(self):
        self.assertTrue(release.validate({}, [], "publisher"))


if __name__ == "__main__":
    unittest.main()
