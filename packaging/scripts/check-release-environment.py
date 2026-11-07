#!/usr/bin/env python3
"""Fail closed unless release-publish has the intended saved protections."""

import argparse
import json
import re
import sys


def validate(environment, policies, custom_rules, tag):
    errors = []
    if environment.get("name") != "release-publish":
        errors.append("release-publish environment is missing")
    if environment.get("can_admins_bypass") is not False:
        errors.append("administrator bypass must be disabled")
    rules = environment.get("protection_rules")
    if not isinstance(rules, list) or any(rule.get("type") != "branch_policy" for rule in rules):
        errors.append("only tag deployment protection is allowed; disable reviewers and wait timers")
    if (type(custom_rules.get("total_count")) is not int or custom_rules["total_count"] != 0
            or custom_rules.get("custom_deployment_protection_rules") != []):
        errors.append("custom deployment protection rules must be disabled")
    branch_policy = environment.get("deployment_branch_policy") or {}
    if branch_policy.get("protected_branches") is not False or branch_policy.get("custom_branch_policies") is not True:
        errors.append("custom tag deployment policy must be enabled without all-branch deployment")
    entries = policies.get("branch_policies")
    if (not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag)
            or type(policies.get("total_count")) is not int or policies["total_count"] != 1
            or not isinstance(entries, list) or len(entries) != 1
            or entries[0].get("type") != "tag" or entries[0].get("name") != tag):
        errors.append(f"deployment must be restricted to the exact release tag: {tag}")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--environment", required=True)
    parser.add_argument("--policies", required=True)
    parser.add_argument("--custom-rules", required=True)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args()
    try:
        environment = json.load(open(args.environment, encoding="utf-8"))
        policies = json.load(open(args.policies, encoding="utf-8"))
        custom_rules = json.load(open(args.custom_rules, encoding="utf-8"))
        errors = validate(environment, policies, custom_rules, args.tag)
    except (OSError, ValueError, TypeError, AttributeError) as exc:
        print(f"cannot verify release environment: {exc}", file=sys.stderr)
        return 1
    if errors:
        print("release environment is not safe to publish:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"release-publish protections verified for {args.tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
