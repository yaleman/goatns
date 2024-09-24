#!/bin/bash

semgrep ci \
		--config auto \
		--junit-xml \
		--output results.xml \
		--exclude-rule "yaml.github-actions.security.third-party-action-not-pinned-to-commit-sha.third-party-action-not-pinned-to-commit-sha" \
		--exclude-rule "python.django.security.django-no-csrf-token.django-no-csrf-token"
