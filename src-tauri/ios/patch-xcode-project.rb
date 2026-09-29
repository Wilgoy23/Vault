#!/usr/bin/env ruby
# Adds the Password AutoFill extension to the Xcode project that
# `tauri ios init` generates in src-tauri/gen/apple, then regenerates it with
# XcodeGen. Run after every `tauri ios init`; running it twice is harmless.
#
#   ruby src-tauri/ios/patch-xcode-project.rb
#
# The app target gains the Swift bridge that src/autofill.rs links against
# (so an unpatched project fails to link on iOS) plus the App Group and
# AutoFill entitlements. The VaultAutoFill extension is added and embedded.

require "json"
require "yaml"

EXTENSION = "VaultAutoFill"
SRC_TAURI = File.expand_path("..", __dir__)
SPEC = File.join(SRC_TAURI, "gen", "apple", "project.yml")
NATIVE = "../../ios" # this directory, relative to gen/apple

abort "#{SPEC} not found; run `npm run tauri ios init` first" unless File.exist?(SPEC)

conf = JSON.parse(File.read(File.join(SRC_TAURI, "tauri.conf.json")))
spec = YAML.load_file(SPEC)
targets = spec.fetch("targets")
app_name, app = targets.find { |name, t| t["type"] == "application" && name != EXTENSION }
abort "No application target in #{SPEC}" unless app

# ── App target ──────────────────────────────────────────────────────────────
sources = (app["sources"] ||= [])
["Shared", "App"].each do |dir|
  path = "#{NATIVE}/#{dir}"
  sources << { "path" => path } unless sources.any? { |s| s.is_a?(Hash) && s["path"] == path }
end
app["entitlements"] = { "path" => "#{NATIVE}/Config/App.entitlements" }
base = ((app["settings"] ||= {})["base"] ||= {})
base["SWIFT_VERSION"] ||= "5.0"
deps = (app["dependencies"] ||= [])
deps << { "target" => EXTENSION } unless deps.any? { |d| d["target"] == EXTENSION }

# ── Extension target ────────────────────────────────────────────────────────
targets[EXTENSION] = {
  "type" => "app-extension",
  "platform" => "iOS",
  "sources" => [{ "path" => "#{NATIVE}/Shared" }, { "path" => "#{NATIVE}/AutoFill" }],
  "settings" => {
    "base" => {
      "PRODUCT_NAME" => EXTENSION,
      "PRODUCT_BUNDLE_IDENTIFIER" => "#{conf.fetch("identifier")}.autofill",
      "INFOPLIST_FILE" => "#{NATIVE}/Config/AutoFill-Info.plist",
      "CODE_SIGN_ENTITLEMENTS" => "#{NATIVE}/Config/AutoFill.entitlements",
      # Must match the app's, or App Store validation rejects the bundle
      "MARKETING_VERSION" => conf.fetch("version"),
      "CURRENT_PROJECT_VERSION" => conf.fetch("version"),
      "SWIFT_VERSION" => "5.0",
      "TARGETED_DEVICE_FAMILY" => "1,2",
      "APPLICATION_EXTENSION_API_ONLY" => "YES",
    },
  },
  "dependencies" => [
    { "sdk" => "AuthenticationServices.framework" },
    { "sdk" => "LocalAuthentication.framework" },
  ],
}

File.write(SPEC, spec.to_yaml)
puts "Added #{EXTENSION} to #{app_name} in #{SPEC}"

exit if ENV["SKIP_XCODEGEN"] # for checking the YAML without a Mac
system("xcodegen", "generate", "--spec", SPEC) or abort "xcodegen failed"
