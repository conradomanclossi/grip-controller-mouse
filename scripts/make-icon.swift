// Draws the GRIP. app icon and writes assets/Grip.icns (plus a 1024px preview PNG).
// Usage: swift scripts/make-icon.swift
//
// Design follows the settings window: light paper, hairline black rules, a heavy
// Helvetica Neue "G", and the orange square from the live stick view as the period.

import AppKit

let paper = NSColor(srgbRed: 0.929, green: 0.929, blue: 0.929, alpha: 1)   // #ededed
let ink = NSColor(srgbRed: 0.043, green: 0.043, blue: 0.043, alpha: 1)     // #0b0b0b
let accent = NSColor(srgbRed: 1.0, green: 0.416, blue: 0.075, alpha: 1)    // #ff6a13

/// Renders the icon at `px` pixels square. All geometry is authored on a 1024 canvas.
func render(_ px: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: px, height: px)
    NSGraphicsContext.saveGraphicsState()
    let ctx = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = ctx
    ctx.imageInterpolation = .high
    let s = CGFloat(px) / 1024
    let t = NSAffineTransform()
    t.scale(by: s)
    t.concat()

    // macOS icon grid: 824pt body inset 100pt, ~185pt corner radius.
    let body = NSRect(x: 100, y: 100, width: 824, height: 824)
    let shape = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)

    // Soft drop shadow, as in Apple's template.
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.28)
    shadow.shadowBlurRadius = 18 * s
    shadow.shadowOffset = NSSize(width: 0, height: -8 * s)
    shadow.set()
    paper.setFill()
    shape.fill()
    NSGraphicsContext.restoreGraphicsState()

    NSGraphicsContext.saveGraphicsState()
    shape.addClip()

    // Masthead rules (hairlines scaled so they stay visible at small sizes).
    let rule = max(6, 1.5 / s)
    ink.setFill()
    NSRect(x: 100, y: 742, width: 824, height: rule).fill()
    NSRect(x: 100, y: 254, width: 824, height: rule).fill()

    // The "G".
    let font = NSFont(name: "HelveticaNeue-Bold", size: 560)!
    let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: ink, .kern: -12]
    let g = NSAttributedString(string: "G", attributes: attrs)
    // Center "G" + period as one group between the rules.
    let gap: CGFloat = 22, dot: CGFloat = 104
    let gWidth = g.size().width
    let x = 512 - (gWidth + gap + dot) / 2
    let baseline: CGFloat = 498 - font.capHeight / 2
    g.draw(at: NSPoint(x: x, y: baseline + font.descender))

    // Orange square period sitting on the baseline.
    accent.setFill()
    NSRect(x: x + gWidth + gap, y: baseline, width: dot, height: dot).fill()

    NSGraphicsContext.restoreGraphicsState()

    // Hairline edge so the light icon doesn't dissolve into light backgrounds.
    NSColor.black.withAlphaComponent(0.12).setStroke()
    shape.lineWidth = max(2, 1 / s)
    shape.stroke()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let fm = FileManager.default
let root = URL(fileURLWithPath: fm.currentDirectoryPath)
let assets = root.appendingPathComponent("assets")
let iconset = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("Grip.iconset")
try? fm.removeItem(at: iconset)
try fm.createDirectory(at: iconset, withIntermediateDirectories: true)
try fm.createDirectory(at: assets, withIntermediateDirectories: true)

for base in [16, 32, 128, 256, 512] {
    try render(base).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try render(base * 2).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
try render(1024).write(to: assets.appendingPathComponent("icon-1024.png"))

let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
p.arguments = ["-c", "icns", iconset.path, "-o", assets.appendingPathComponent("Grip.icns").path]
try p.run()
p.waitUntilExit()
guard p.terminationStatus == 0 else { fatalError("iconutil failed") }
print("wrote assets/Grip.icns and assets/icon-1024.png")
