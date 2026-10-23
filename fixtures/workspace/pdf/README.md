# Synthetic protected PDFs

These contain only the public `pdfFixture("text")` statement from
`scripts/workspace-pdf-fixtures.mjs`. They were encrypted with pypdf 6.21.2 using
`PdfWriter.append` and `PdfWriter.encrypt(..., algorithm="RC4-128")`.

- `password.pdf`: user password `synthetic-only`.
- `empty-user-password.pdf`: empty user password; opens without prompting but is
  still encrypted and must be rejected.

Both use owner password `synthetic-owner-only`. These test encryption refusal,
not cipher strength. Python/pypdf is not a test runtime dependency.
