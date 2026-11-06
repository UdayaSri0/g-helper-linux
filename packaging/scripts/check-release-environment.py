#!/usr/bin/env python3
"""Fail closed unless release-publish has the intended saved protections."""

import argparse
import json
import sys


def validate(environment, policies, actor):
    errors = []
    policies = policies.get("branch_policies", []) if isinstance(policies, dict) else policies
    if environment.get("name") != "release-publish":
        errors.append("release-publish environment is missing")
    if environment.get("can_admins_bypass") is not False:
        errors.append("administrator bypass must be disabled")
    reviewers = [rule for rule in environment.get("protection_rules", []) if rule.get("type") == "required_reviewers"]
    if len(reviewers) != 1 or not reviewers[0].get("reviewers"):
        errors.append("exactly one non-empty required-reviewer rule is required")
    else:
        rule = reviewers[0]
        if rule.get("prevent_self_review") is not True:
            errors.append("prevent_self_review must be enabled")
        allowed = {str((person.get("reviewer") or {}).get("login", person.get("login", ""))).lower()
                   for person in rule["reviewers"] if person.get("type") == "User"}
        if actor.lower() in allowed:
            errors.append("workflow actor is a configured reviewer and cannot independently approve")
    branch_policy = environment.get("deployment_branch_policy") or {}
    if branch_policy.get("protected_branches") is not False or branch_policy.get("custom_branch_policies") is not True:
        errors.append("custom tag deployment policy must be enabled without all-branch deployment")
    if len(policies) != 1 or policies[0].get("type") != "tag" or policies[0].get("name") != "v0.4.1":
        errors.append("deployment must be restricted to the v0.4.1 tag")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--environment", required=True)
    parser.add_argument("--policies", required=True)
    parser.add_argument("--actor", required=True)
    args = parser.parse_args()
    try:
        environment = json.load(open(args.environment, encoding="utf-8"))
        policies = json.load(open(args.policies, encoding="utf-8"))
        errors = validate(environment, policies, args.actor)
    except (OSError, ValueError, TypeError) as exc:
        print(f"cannot verify release environment: {exc}", file=sys.stderr)
        return 1
    if errors:
        print("release environment is not safe to publish:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("release-publish protections verified for v0.4.1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
