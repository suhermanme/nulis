cask "nulis" do
  version "0.5.0"
  sha256 "120d64addc441e4263d7835c7cf382a74700baf549ca90e3848e24d41da43194"

  url "https://github.com/suhermanme/nulis/releases/download/v#{version}/Nulis_#{version}_universal.dmg"
  name "Nulis"
  desc "Editor for text, source code, hexadecimal data, and files larger than memory"
  homepage "https://github.com/suhermanme/nulis"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on :macos

  app "Nulis.app"

  zap trash: [
    "~/Library/Application Support/com.nulis.editor",
    "~/Library/Caches/com.nulis.editor",
    "~/Library/Preferences/com.nulis.editor.plist",
    "~/Library/Saved Application State/com.nulis.editor.savedState",
    "~/Library/WebKit/com.nulis.editor",
  ]

  caveats <<~EOS
    Nulis is not notarized by Apple, so macOS blocks it the first time it opens.
    Allow it once in System Settings > Privacy & Security > Open Anyway:
      https://github.com/suhermanme/nulis#macos
  EOS
end
