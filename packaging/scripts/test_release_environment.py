import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("release_environment", Path(__file__).with_name("check-release-environment.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


def configured(tag="v0.4.1"):
    return ({"name": "release-publish", "can_admins_bypass": False,
             "protection_rules": [{"type": "branch_policy"}],
             "deployment_branch_policy": {"protected_branches": False, "custom_branch_policies": True}},
            {"total_count": 1, "branch_policies": [{"type": "tag", "name": tag}]},
            {"total_count": 0, "custom_deployment_protection_rules": []})


class ReleaseEnvironmentTests(unittest.TestCase):
    def test_accepts_solo_policy_for_current_and_future_tags(self):
        for tag in ("v0.4.1", "v0.5.0"):
            self.assertEqual(release.validate(*configured(tag), tag), [])

    def test_rejects_invalid_saved_settings(self):
        for key, value in (("name", "other"), ("can_admins_bypass", True),
                           ("can_admins_bypass", None), ("deployment_branch_policy", None),
                           ("protection_rules", None)):
            with self.subTest(key=key, value=value):
                env, policies, custom = configured()
                env[key] = value
                self.assertTrue(release.validate(env, policies, custom, "v0.4.1"))

    def test_rejects_unexpected_gates(self):
        for rule in ("required_reviewers", "wait_timer", "custom"):
            env, policies, custom = configured()
            env["protection_rules"].append({"type": rule})
            self.assertTrue(release.validate(env, policies, custom, "v0.4.1"), rule)

    def test_rejects_enabled_custom_rules(self):
        env, policies, custom = configured()
        custom.update(total_count=1, custom_deployment_protection_rules=[{"id": 1}])
        self.assertTrue(release.validate(env, policies, custom, "v0.4.1"))

    def test_rejects_inexact_or_extra_tag_rules(self):
        for name, kind, count in (("v*", "tag", 1), ("v0.4.0", "tag", 1),
                                  ("v0.4.1", "branch", 1), ("v0.4.1", "tag", 2)):
            env, policies, custom = configured(name)
            policies["branch_policies"][0]["type"] = kind
            policies["total_count"] = count
            self.assertTrue(release.validate(env, policies, custom, "v0.4.1"))
        self.assertTrue(release.validate(*configured("v*"), "v*"))


if __name__ == "__main__":
    unittest.main()
