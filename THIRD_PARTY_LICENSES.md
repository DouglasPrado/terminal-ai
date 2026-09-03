# Third-party licenses

Terminal AI redistributes the following third-party software inside its application bundle.

---

## ai-memory

- **Project**: <https://github.com/akitaonrails/ai-memory>
- **Version shipped**: pinned in [`scripts/ai-memory.lock`](./scripts/ai-memory.lock)
- **License**: MIT
- **How it is used**: shipped as a Tauri sidecar binary (`bundle.externalBin`) and run as a
  loopback-only child process. It is Terminal AI's memory kernel — see
  [`specs/002-ai-memory-kernel/`](./specs/002-ai-memory-kernel/).

The full license text is redistributed with the application at
`resources/third-party/ai-memory-LICENSE.txt`, and is reproduced below. The MIT license requires
that this copyright notice travel with every copy of the software, which is why the fetch script
copies it out of the release archive rather than leaving it to be remembered by hand.

```text
MIT License

Copyright (c) 2026 Fabio Akita

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
