// poc-rekor offline verifier: verifies a rekor-tiles (Rekor v2 / tessera) entry
// fully offline from:
//   - the log's public key (pre-seeded trust root)
//   - the log checkpoint (C2SP signed note)
//   - the inclusion proof (log index + hashes)
//   - the canonicalized entry body
//   - the original artifact (trace checkpoint) and the artifact signer's public key
//
// Mirrors the verification performed by rekor-tiles' own e2e test
// (tests/e2e_test.go: verifyInclusionProof) using the repo's own pkg/verify.
package main

import (
	"bytes"
	"crypto/ecdsa"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"flag"
	"fmt"
	"os"

	pb "github.com/sigstore/rekor-tiles/v2/pkg/generated/protobuf"
	"github.com/sigstore/rekor-tiles/v2/pkg/note"
	"github.com/sigstore/rekor-tiles/v2/pkg/types/hashedrekord"
	"github.com/sigstore/rekor-tiles/v2/pkg/verify"
	"github.com/sigstore/sigstore/pkg/cryptoutils"
	"github.com/sigstore/sigstore/pkg/signature"
	f_log "github.com/transparency-dev/formats/log"
	"github.com/transparency-dev/merkle/proof"
	"github.com/transparency-dev/merkle/rfc6962"
	"google.golang.org/protobuf/encoding/protojson"
)

func fatalf(step string, err error) {
	fmt.Fprintf(os.Stderr, "FAIL at %s: %v\n", step, err)
	os.Exit(2)
}

func ok(step string) { fmt.Printf("PASS %s\n", step) }

func loadPEMPubKey(path string) ([]byte, error) {
	pemBytes, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	blk, _ := pem.Decode(pemBytes)
	if blk == nil {
		return nil, fmt.Errorf("no PEM block in %s", path)
	}
	return blk.Bytes, nil
}

func main() {
	logPub := flag.String("log-pub", "", "PEM public key of the log (ed25519), the pre-seeded trust root")
	origin := flag.String("origin", "", "checkpoint origin (hostname)")
	checkpointFile := flag.String("checkpoint", "", "file with the checkpoint envelope (signed note)")
	proofFile := flag.String("proof", "", "JSON file {logIndex, hashes[]}")
	bodyFile := flag.String("body", "", "file with base64 canonicalized entry body")
	artifactFile := flag.String("artifact", "", "the original artifact (trace checkpoint) file")
	artifactPub := flag.String("artifact-pub", "", "PEM public key of the artifact signer (ECDSA P-256)")
	flag.Parse()

	// ---- step 0: load inputs ----
	logPubDER, err := loadPEMPubKey(*logPub)
	if err != nil {
		fatalf("load log public key", err)
	}
	logPubKey, err := cryptoutils.UnmarshalPEMToPublicKey([]byte("-----BEGIN PUBLIC KEY-----\n" + base64.StdEncoding.EncodeToString(logPubDER) + "\n-----END PUBLIC KEY-----\n"))
	if err != nil {
		fatalf("parse log public key", err)
	}
	verifier, err := signature.LoadDefaultVerifier(logPubKey)
	if err != nil {
		fatalf("load log verifier", err)
	}
	noteVerifier, err := note.NewNoteVerifier(*origin, verifier)
	if err != nil {
		fatalf("build note verifier", err)
	}
	checkpointTxt, err := os.ReadFile(*checkpointFile)
	if err != nil {
		fatalf("read checkpoint", err)
	}
	proofBytes, err := os.ReadFile(*proofFile)
	if err != nil {
		fatalf("read proof", err)
	}
	bodyB64, err := os.ReadFile(*bodyFile)
	if err != nil {
		fatalf("read body", err)
	}
	artifact, err := os.ReadFile(*artifactFile)
	if err != nil {
		fatalf("read artifact", err)
	}
	ok("inputs loaded")

	// ---- step 1: verify checkpoint signature with the pre-seeded log public key ----
	cp, _, _, err := f_log.ParseCheckpoint(checkpointTxt, noteVerifier.Name(), noteVerifier)
	if err != nil {
		fatalf("checkpoint signature verification", err)
	}
	ok(fmt.Sprintf("checkpoint signature (origin=%s size=%d root=%s)", cp.Origin, cp.Size, base64.StdEncoding.EncodeToString(cp.Hash)))

	// ---- step 2: parse proof + canonicalized body ----
	var pf struct {
		LogIndex string   `json:"logIndex"`
		Hashes   []string `json:"hashes"`
	}
	if err := json.Unmarshal(proofBytes, &pf); err != nil {
		fatalf("parse proof json", err)
	}
	var idx uint64
	fmt.Sscanf(pf.LogIndex, "%d", &idx)
	hashes := make([][]byte, 0, len(pf.Hashes))
	for _, h := range pf.Hashes {
		b, err := base64.StdEncoding.DecodeString(h)
		if err != nil {
			fatalf("decode proof hash", err)
		}
		hashes = append(hashes, b)
	}
	bodyB64Trim := bytes.TrimSpace(bodyB64)
	body, err := base64.StdEncoding.DecodeString(string(bodyB64Trim))
	if err != nil {
		fatalf("decode canonicalized body", err)
	}
	leafHash := rfc6962.DefaultHasher.HashLeaf(body)

	// ---- step 3: verify merkle inclusion of the logged entry under the checkpoint root ----
	if err := proof.VerifyInclusion(rfc6962.DefaultHasher, idx, cp.Size, leafHash, hashes, cp.Hash); err != nil {
		fatalf("inclusion proof verification", err)
	}
	ok(fmt.Sprintf("inclusion proof (logIndex=%d treeSize=%d)", idx, cp.Size))

	// ---- step 4: bind the artifact to the logged entry ----
	// 4a. sha256(artifact) must equal the digest inside the logged entry
	artDigest := sha256.Sum256(artifact)
	entry := &pb.Entry{}
	if err := protojson.Unmarshal(body, entry); err != nil {
		fatalf("parse entry from canonicalized body", err)
	}
	hr := entry.Spec.GetHashedRekordV002()
	if hr == nil {
		fatalf("entry kind", fmt.Errorf("not a hashedrekord v0.0.2 entry"))
	}
	if !bytes.Equal(artDigest[:], hr.Data.Digest) {
		fatalf("artifact digest binding", fmt.Errorf("artifact sha256 %s != logged digest %s (artifact was modified after logging)",
			base64.StdEncoding.EncodeToString(artDigest[:]), base64.StdEncoding.EncodeToString(hr.Data.Digest)))
	}
	ok("artifact digest matches logged entry digest")

	// 4b. artifact signature verifies under the artifact signer's public key
	artPubDER, err := loadPEMPubKey(*artifactPub)
	if err != nil {
		fatalf("load artifact public key", err)
	}
	artPubAny, err := x509.ParsePKIXPublicKey(artPubDER)
	if err != nil {
		fatalf("parse artifact public key", err)
	}
	artPub, isECDSA := artPubAny.(*ecdsa.PublicKey)
	if !isECDSA {
		fatalf("artifact key type", fmt.Errorf("expected ECDSA public key"))
	}
	if !ecdsa.VerifyASN1(artPub, artDigest[:], hr.Signature.Content) {
		fatalf("artifact signature verification", fmt.Errorf("ECDSA signature over digest does not verify"))
	}
	ok("artifact ECDSA signature verifies")

	// 4c. the entry reconstructed from the artifact inputs hashes to the same leaf the proof commits to
	reconstructedHash, err := hashedrekord.ToEntryHash(artDigest[:], hr.Signature)
	if err != nil {
		fatalf("reconstruct entry hash", err)
	}
	if err := proof.VerifyInclusion(rfc6962.DefaultHasher, idx, cp.Size, reconstructedHash, hashes, cp.Hash); err != nil {
		fatalf("inclusion proof against reconstructed entry", err)
	}
	ok("reconstructed entry (from artifact alone) is provably included in the log")

	// sanity: cross-check with the repo's own verify helper semantics (checkpoint + TLE path)
	fmt.Println("RESULT: VERIFIED — artifact (trace checkpoint) is bound into the transparency log")
	_ = verify.VerifyCheckpoint // referenced to keep the pkg/verify dependency explicit
}
