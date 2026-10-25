.PHONY: check core-lint core-test core-doc bridge swift-lint swift-format swift-test app-project app

CORE := core
KIT := apps/macos/Packages/ClientMailKit
# Handwritten Swift only: generated bindings are excluded.
SWIFT_SOURCES := apps/macos/ClientMail $(KIT)/Package.swift \
	$(KIT)/Sources/MailDomain $(KIT)/Sources/CoreAdapter $(KIT)/Sources/AccountsFeature $(KIT)/Tests
SWIFT_FORMAT_CONFIG := --configuration apps/macos/.swift-format

check: core-lint core-test core-doc bridge swift-lint swift-test app

core-lint:
	cd $(CORE) && cargo fmt --all --check
	cd $(CORE) && cargo clippy --workspace --all-targets -- -D warnings

core-test:
	cd $(CORE) && cargo test --workspace

core-doc:
	cd $(CORE) && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

bridge:
	scripts/build-core-bridge.sh

swift-lint:
	xcrun swift-format lint $(SWIFT_FORMAT_CONFIG) --strict --recursive $(SWIFT_SOURCES)

swift-format:
	xcrun swift-format format $(SWIFT_FORMAT_CONFIG) --in-place --recursive $(SWIFT_SOURCES)

swift-test: bridge
	cd $(KIT) && swift test

app-project:
	cd apps/macos && xcodegen generate --quiet

app: bridge app-project
	xcodebuild -project apps/macos/ClientMail.xcodeproj -scheme ClientMail \
		-destination 'platform=macOS,arch=arm64' -derivedDataPath apps/macos/build \
		-quiet build
