/**
 * @generated SignedSource<<test>>
 * @lightSyntaxTransform
 * @nogrep
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import type { ConcreteRequest } from "relay-runtime";
export type FileOne$variables = { id: string };
export type FileOne$data = {};
export type FileOne = {
  variables: FileOne$variables;
  response: FileOne$data;
};

const node: ConcreteRequest = {
  "fragment": {
    "argumentDefinitions": [
      {
        "defaultValue": null,
        "kind": "LocalArgument",
        "name": "id"
      }
    ],
    "kind": "Fragment",
    "name": "FileOne",
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "unique_only_in_file_one",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id_field_in_all_3_files",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "name_field_in_all_3_files",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "field_in_files_1_and_2",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "Connection",
        "kind": "LinkedField",
        "name": "items",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "concreteType": "Edge",
            "kind": "LinkedField",
            "name": "edges",
            "plural": true,
            "selections": [
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "cursor_in_all_3_files",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "concreteType": "Node",
                "kind": "LinkedField",
                "name": "node",
                "plural": false,
                "selections": [
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "id_field_in_all_3_files",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "name_field_in_all_3_files",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": [
                      {
                        "kind": "Literal",
                        "name": "single_arg_appears_3x_NOT_array_extracted",
                        "value": "only_one"
                      }
                    ],
                    "kind": "ScalarField",
                    "name": "singleArgField",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": [
                      {
                        "kind": "Literal",
                        "name": "multi_arg_A_appears_3x",
                        "value": "first"
                      },
                      {
                        "kind": "Literal",
                        "name": "multi_arg_B_appears_3x",
                        "value": "second"
                      }
                    ],
                    "kind": "ScalarField",
                    "name": "multiArgField",
                    "storageKey": null
                  }
                ],
                "storageKey": null
              }
            ],
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "concreteType": "PageInfo",
            "kind": "LinkedField",
            "name": "pageInfo",
            "plural": false,
            "selections": [
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "hasNextPage_in_all_3",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "endCursor_in_all_3",
                "storageKey": null
              }
            ],
            "storageKey": null
          }
        ],
        "storageKey": null
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      {
        "defaultValue": null,
        "kind": "LocalArgument",
        "name": "id"
      }
    ],
    "kind": "Operation",
    "name": "FileOne",
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id_field_in_all_3_files",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "name_field_in_all_3_files",
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "abc123",
    "id": null,
    "metadata": {},
    "name": "FileOne",
    "operationKind": "query",
    "text": "query FileOne { ... }"
  }
};

(node as any).hash = "hash1";

export default node;
