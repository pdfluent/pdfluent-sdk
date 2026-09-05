class Pdfluent < Formula
  desc "High-performance PDF processing CLI — PDF/A, XFA, text extraction"
  homepage "https://pdfluent.com"
  version "1.0.0-beta.1"
  license "Proprietary"

  on_macos do
    on_arm do
      url "https://github.com/pdfluent/engine/releases/download/v#{version}/xfa-cli-aarch64-apple-darwin-v#{version}.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/pdfluent/engine/releases/download/v#{version}/xfa-cli-x86_64-apple-darwin-v#{version}.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "xfa-cli" => "pdfluent"
  end

  test do
    system "#{bin}/pdfluent", "--version"
  end
end
