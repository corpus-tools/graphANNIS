# Introduction

The graphANNIS library is a new backend implementation of the [ANNIS linguistic search and visualization system](http://corpus-tools.org/annis/).

It is part of the larger system with a web-based front-end, a REST-service (both written in the Java programming language).
![graphANNIS architecture overview](images/graphannis-architecture.png)
As a backend, it is in charge of performing the actual AQL queries and returning the results, which can be either
the number of matches, the IDs of the matches or sub-graphs for a specific set of matches.

## Crates

GraphANNIS currently consists of the following sub-crates:

- graphannis-core (`core/`): Generic functionality for representing, storing and querying a generic property graph.
- graphannis (`graphannis/`): The complete library with support for linguistic corpora and AQL
- graphannis-cli (`cli/`) : A command line interface to e.g. import corpora or search them.
- graphannis-capi (`cli/`) : A C-API for graphANNIS.
- graphannis-tutorial (`examples/tutorial`): An example how to use the API.
 