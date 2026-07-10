# Introduction

This document contains a collection of notes, ideas, and development tasks related to improving the bot’s citation-handling capabilities.

The primary goal is for the bot to recognize the type of source provided and determine the most appropriate citation format. For example, when a user submits a DOI for an academic article, the bot should identify the source, retrieve the relevant metadata, and generate a correctly formatted BibTeX entry for use in reports and other academic documents.

The long-term aim is to create a bot that can keep track of sources shared within a server, organize them in a useful way, and potentially integrate more closely with our Typst reports. This could make it easier to collect references, maintain bibliographies, and connect discussions directly to written documentation.

## Current Features

- **DOI articles**
  - Supports full DOI links, such as `https://doi.org/10.1371/journal.pbio.0040157`
  - Supports bare DOI identifiers, such as `10.1371/journal.pbio.0040157`

- **arXiv articles**
  - Supports arXiv abstract links, such as `https://arxiv.org/abs/1706.03762`

## Wish list

- **Direct IEEE Xplore links**
  - Detect `ieeexplore.ieee.org/document/...` links and resolve their associated DOI

- **Direct ACM Digital Library links**
  - Detect `dl.acm.org/doi/...` links and extract the DOI automatically

- **RFC documents**
  - Support RFC Editor and IETF links, such as `rfc-editor.org/rfc/rfc...`

- **GitHub and software releases**
  - Generate citations for repositories, tagged releases, and archived versions

- **Hardware datasheets and technical manuals**
  - Generate citations from manufacturer documentation and PDF metadata

- **Standards**
  - Support IEEE, ISO, IEC, NIST, W3C, and ECMA standard identifiers and links

## Quick Notes

There are several types of metadata that can be used to generate accurate citations, along with multiple standards and databases for retrieving it.

- **DOI metadata** is often the most reliable option for published academic articles.
- **Dublin Core** is a widely used metadata standard that can provide useful bibliographic information.
- **Standard HTML metadata** can also be surprisingly effective, especially when a page includes structured citation tags such as `citation_title`, `citation_author`, and `citation_publication_date`.
