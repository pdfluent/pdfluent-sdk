cask "pdfluent" do
  version "1.0.0-beta.21"
  sha256 "afb72601c7ede6e2359db58478c2fe49e8f86d61d91c9c15972421f5e738b807"

  url "https://pdfluent.com/releases/#{version}/PDFluent_#{version}_universal.dmg"
  name "PDFluent"
  desc "PDF editor that processes documents on the machine it runs on"
  homepage "https://pdfluent.com/"

  # The published artefact is one universal binary (x86_64 + arm64), so there is
  # no per-architecture url and no `arch` stanza. `lipo -archs` on the bundle
  # executable of this very build reports both.
  livecheck do
    url "https://pdfluent.com/releases/latest.json"
    strategy :json do |json|
      json["version"]
    end
  end

  # The application ships the Tauri updater and replaces itself in place, so
  # Homebrew must not report an out-of-date version it did not install. Without
  # this, `brew upgrade` fights the updater over the same bundle.
  auto_updates true
  depends_on macos: :big_sur

  app "PDFluent.app"

  # `zap` removes what the application writes under the user's Library; the
  # bundle identifier is com.pdfluent.app, which is what Tauri derives every one
  # of these paths from.
  zap trash: [
    "~/Library/Application Support/com.pdfluent.app",
    "~/Library/Caches/com.pdfluent.app",
    "~/Library/HTTPStorages/com.pdfluent.app",
    "~/Library/Preferences/com.pdfluent.app.plist",
    "~/Library/Saved Application State/com.pdfluent.app.savedState",
    "~/Library/WebKit/com.pdfluent.app",
  ]
end
