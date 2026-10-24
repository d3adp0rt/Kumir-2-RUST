# Convenience wrapper around scripts/build.sh.
#
#   make help          — список целей
#   make linux         — собрать Linux-пакет (tar.gz)
#   make windows       — собрать Windows-пакет (zip)
#   make apk           — собрать подписанный APK
#   make release       — linux + windows + apk
#   make test          — прогнать тесты
#
# Все подробности и переменные окружения — в docs/BUILD.md.

SHELL := /bin/sh
BUILD := bash scripts/build.sh
ARGS ?=

.DEFAULT_GOAL := help

.PHONY: help all linux windows apk android release test check fmt fmt-check clippy run cli clean distclean doctor

help:
	@$(BUILD) help

all: linux

linux:
	@$(BUILD) linux

windows:
	@$(BUILD) windows

apk android:
	@$(BUILD) apk

release: linux windows apk

test:
	@$(BUILD) test $(ARGS)

check:
	@$(BUILD) check

fmt:
	@$(BUILD) fmt

fmt-check:
	@$(BUILD) fmt-check

clippy:
	@$(BUILD) clippy

run:
	@$(BUILD) run $(ARGS)

cli:
	@$(BUILD) cli $(ARGS)

clean:
	@$(BUILD) clean

distclean:
	@$(BUILD) distclean

doctor:
	@$(BUILD) doctor
