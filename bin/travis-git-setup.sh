#!/usr/bin/env bash

# setup git
git init
git config --global user.email "lynncyrin@gmail.com"
git config --global user.name "[[ BOT ]] Lynn Cyrin"
git config --global pull.rebase true
git remote remove origin
git remote add origin https://${GITHUB_API_TOKEN}@github.com/${TRAVIS_REPO_SLUG}.git
