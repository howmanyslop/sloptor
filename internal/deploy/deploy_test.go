package deploy

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"rotor/internal/cloud"
	"rotor/internal/config"
)

// fakeCloud is an in-memory CloudClient: every method records its call and
// returns scripted results. No network.
type fakeCloud struct {
	failPublish map[int64]error // placeID -> error
	publishes   []publishCall

	universeMasks   [][]string
	universePatches []cloud.Universe

	placeMasks [][]string

	failCreateBadge error
	nextBadgeID     int64
	createdBadges   []cloud.CreateBadgeRequest
	updatedBadges   []badgeUpdate

	failCreateAsset error
	nextAssetID     int64
	createdAssets   []cloud.CreateAssetRequest

	nextGamePassID    int64
	createdGamePasses []cloud.CreateGamePassRequest
	updatedGamePasses []gamePassUpdate

	nextIconID  int64
	iconUploads []string // file names

	failUploadThumbnail error
	nextThumbnailID     int64
	thumbnailUploads    []string // file names
	thumbnailOrders     [][]int64
	deletedThumbnails   []int64

	nextProductID   int64
	createdProducts []cloud.CreateDeveloperProductRequest
	updatedProducts []productUpdate

	nextLinkID   int64
	createdLinks []cloud.SocialLinkRequest
	updatedLinks []linkUpdate
	deletedLinks []int64
}

type publishCall struct {
	UniverseID, PlaceID int64
	VersionType         string
	Bytes               int
}

type badgeUpdate struct {
	ID  int64
	Req cloud.UpdateBadgeRequest
}

type gamePassUpdate struct {
	ID  int64
	Req cloud.UpdateGamePassRequest
}

type productUpdate struct {
	ID  int64
	Req cloud.UpdateDeveloperProductRequest
}

type linkUpdate struct {
	ID  int64
	Req cloud.SocialLinkRequest
}

func (f *fakeCloud) UpdateUniverse(ctx context.Context, universeID int64, u cloud.Universe, mask []string) (cloud.Universe, error) {
	f.universePatches = append(f.universePatches, u)
	f.universeMasks = append(f.universeMasks, mask)
	return u, nil
}

func (f *fakeCloud) UpdatePlace(ctx context.Context, universeID, placeID int64, p cloud.Place, mask []string) (cloud.Place, error) {
	f.placeMasks = append(f.placeMasks, mask)
	return p, nil
}

func (f *fakeCloud) PublishPlaceVersion(ctx context.Context, universeID, placeID int64, versionType string, body io.Reader) (int64, error) {
	if err := f.failPublish[placeID]; err != nil {
		return 0, err
	}
	data, _ := io.ReadAll(body)
	f.publishes = append(f.publishes, publishCall{universeID, placeID, versionType, len(data)})
	return int64(len(f.publishes)), nil
}

func (f *fakeCloud) CreateBadge(ctx context.Context, universeID int64, req cloud.CreateBadgeRequest) (cloud.Badge, error) {
	if f.failCreateBadge != nil {
		return cloud.Badge{}, f.failCreateBadge
	}
	f.nextBadgeID++
	f.createdBadges = append(f.createdBadges, req)
	return cloud.Badge{ID: 500 + f.nextBadgeID, Name: req.Name}, nil
}

func (f *fakeCloud) UpdateBadge(ctx context.Context, badgeID int64, req cloud.UpdateBadgeRequest) (cloud.Badge, error) {
	f.updatedBadges = append(f.updatedBadges, badgeUpdate{badgeID, req})
	return cloud.Badge{ID: badgeID, Name: req.Name}, nil
}

func (f *fakeCloud) CreateAsset(ctx context.Context, req cloud.CreateAssetRequest, fileName string, file io.Reader) (string, error) {
	if f.failCreateAsset != nil {
		return "", f.failCreateAsset
	}
	f.nextAssetID++
	f.createdAssets = append(f.createdAssets, req)
	return fmt.Sprintf("operations/%d", f.nextAssetID), nil
}

func (f *fakeCloud) PollOperation(ctx context.Context, path string, into any) error {
	if a, ok := into.(*cloud.Asset); ok {
		var n int64
		if _, err := fmt.Sscanf(path, "operations/%d", &n); err != nil {
			return fmt.Errorf("fakeCloud: parsing operation path %q: %w", path, err)
		}
		a.AssetID = 700 + n
	}
	return nil
}

func (f *fakeCloud) CreateGamePass(ctx context.Context, universeID int64, req cloud.CreateGamePassRequest) (cloud.GamePass, error) {
	f.nextGamePassID++
	f.createdGamePasses = append(f.createdGamePasses, req)
	return cloud.GamePass{GamePassID: 300 + f.nextGamePassID, Name: req.Name}, nil
}

func (f *fakeCloud) UpdateGamePass(ctx context.Context, gamePassID int64, req cloud.UpdateGamePassRequest) (cloud.GamePass, error) {
	f.updatedGamePasses = append(f.updatedGamePasses, gamePassUpdate{gamePassID, req})
	return cloud.GamePass{GamePassID: gamePassID, Name: req.Name}, nil
}

func (f *fakeCloud) UploadUniverseIcon(ctx context.Context, universeID int64, fileName string, file io.Reader) (int64, error) {
	f.nextIconID++
	f.iconUploads = append(f.iconUploads, fileName)
	return 800 + f.nextIconID, nil
}

func (f *fakeCloud) UploadUniverseThumbnail(ctx context.Context, universeID int64, fileName string, file io.Reader) (int64, error) {
	if f.failUploadThumbnail != nil {
		return 0, f.failUploadThumbnail
	}
	f.nextThumbnailID++
	f.thumbnailUploads = append(f.thumbnailUploads, fileName)
	return 900 + f.nextThumbnailID, nil
}

func (f *fakeCloud) SetUniverseThumbnailOrder(ctx context.Context, universeID int64, ids []int64) error {
	f.thumbnailOrders = append(f.thumbnailOrders, append([]int64(nil), ids...))
	return nil
}

func (f *fakeCloud) DeleteUniverseThumbnail(ctx context.Context, universeID, thumbnailID int64) error {
	f.deletedThumbnails = append(f.deletedThumbnails, thumbnailID)
	return nil
}

func (f *fakeCloud) CreateDeveloperProduct(ctx context.Context, universeID int64, req cloud.CreateDeveloperProductRequest) (cloud.DeveloperProduct, error) {
	f.nextProductID++
	f.createdProducts = append(f.createdProducts, req)
	return cloud.DeveloperProduct{ID: 600 + f.nextProductID, Name: req.Name}, nil
}

func (f *fakeCloud) UpdateDeveloperProduct(ctx context.Context, universeID, productID int64, req cloud.UpdateDeveloperProductRequest) (cloud.DeveloperProduct, error) {
	f.updatedProducts = append(f.updatedProducts, productUpdate{productID, req})
	return cloud.DeveloperProduct{ID: productID, Name: req.Name}, nil
}

func (f *fakeCloud) CreateSocialLink(ctx context.Context, universeID int64, req cloud.SocialLinkRequest) (cloud.SocialLink, error) {
	f.nextLinkID++
	f.createdLinks = append(f.createdLinks, req)
	return cloud.SocialLink{ID: 400 + f.nextLinkID, Title: req.Title}, nil
}

func (f *fakeCloud) UpdateSocialLink(ctx context.Context, universeID, linkID int64, req cloud.SocialLinkRequest) (cloud.SocialLink, error) {
	f.updatedLinks = append(f.updatedLinks, linkUpdate{linkID, req})
	return cloud.SocialLink{ID: linkID, Title: req.Title}, nil
}

func (f *fakeCloud) DeleteSocialLink(ctx context.Context, universeID, linkID int64) error {
	f.deletedLinks = append(f.deletedLinks, linkID)
	return nil
}

// Compile-time check that the fake stays in sync with the provider-facing
// interface.
var _ CloudClient = (*fakeCloud)(nil)

// --- canonical hashing ---

func TestCanonicalHashStability(t *testing.T) {
	// Struct and equivalent map hash identically; key order never matters.
	in := PlaceFileInputs{PlaceID: 42, File: "game.rbxl", FileHash: "sha256:ab", VersionType: "Published"}
	h1, err := HashInputs(in)
	if err != nil {
		t.Fatal(err)
	}
	h2, err := HashInputs(map[string]any{
		"versionType": "Published",
		"placeId":     42,
		"fileHash":    "sha256:ab",
		"file":        "game.rbxl",
	})
	if err != nil {
		t.Fatal(err)
	}
	if h1 != h2 {
		t.Fatalf("hash differs for equivalent inputs:\n%s\n%s", h1, h2)
	}
	if want := "sha256:"; h1[:7] != want {
		t.Fatalf("hash prefix = %q, want %q", h1[:7], want)
	}
	// Any field change must change the hash.
	in.FileHash = "sha256:cd"
	h3, _ := HashInputs(in)
	if h3 == h1 {
		t.Fatal("hash unchanged after input change")
	}
}

// --- state ---

func TestStateRoundTrip(t *testing.T) {
	dir := t.TempDir()
	path := StatePath(dir, "dev")

	st, err := LoadState(path)
	if err != nil {
		t.Fatal(err)
	}
	if len(st.Resources) != 0 {
		t.Fatalf("fresh state not empty: %v", st.Resources)
	}

	st.Resources["badge/winner"] = &StateEntry{
		InputsHash: "sha256:abc",
		Outputs:    map[string]any{"badgeId": int64(123)},
	}
	if err := st.Save(path); err != nil {
		t.Fatal(err)
	}

	loaded, err := LoadState(path)
	if err != nil {
		t.Fatal(err)
	}
	e := loaded.Resources["badge/winner"]
	if e == nil || e.InputsHash != "sha256:abc" {
		t.Fatalf("round-trip lost entry: %+v", e)
	}
	id, ok := OutputInt64(e.Outputs["badgeId"])
	if !ok || id != 123 {
		t.Fatalf("badgeId after reload = %v (%v)", e.Outputs["badgeId"], ok)
	}
}

// --- plan diffing ---

func TestBuildPlanTable(t *testing.T) {
	keep := Resource{Kind: KindPlaceFile, Name: "keep", Inputs: PlaceFileInputs{PlaceID: 1, FileHash: "sha256:a"}}
	change := Resource{Kind: KindPlaceFile, Name: "change", Inputs: PlaceFileInputs{PlaceID: 2, FileHash: "sha256:b"}}
	fresh := Resource{Kind: KindBadge, Name: "new", Inputs: BadgeInputs{Name: "New"}}

	keepHash, _ := HashInputs(keep.Inputs)
	st := NewState()
	st.Resources["place_file/keep"] = &StateEntry{InputsHash: keepHash}
	st.Resources["place_file/change"] = &StateEntry{InputsHash: "sha256:stale"}
	st.Resources["experience/old"] = &StateEntry{InputsHash: "sha256:gone"}

	plan, err := BuildPlan([]Resource{keep, change, fresh}, st, PlanOptions{})
	if err != nil {
		t.Fatal(err)
	}
	got := map[string]Op{}
	for _, s := range plan.Steps {
		got[s.Ref.Key()] = s.Op
	}
	want := map[string]Op{
		"place_file/keep":   OpNoop,
		"place_file/change": OpUpdate,
		"badge/new":         OpCreate,
		"experience/old":    OpBlockedDelete,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("plan ops = %v, want %v", got, want)
	}
	if plan.Creates != 1 || plan.Updates != 1 || plan.Noops != 1 || plan.BlockedDeletes != 1 || plan.Deletes != 0 {
		t.Fatalf("tallies: %+v", plan)
	}

	// With AllowDeletes the blocked delete becomes a real delete.
	plan2, err := BuildPlan([]Resource{keep, change, fresh}, st, PlanOptions{AllowDeletes: true})
	if err != nil {
		t.Fatal(err)
	}
	last := plan2.Steps[len(plan2.Steps)-1]
	if last.Op != OpDelete || last.Ref.Key() != "experience/old" {
		t.Fatalf("delete step = %+v", last)
	}
	if plan2.Deletes != 1 || plan2.BlockedDeletes != 0 {
		t.Fatalf("tallies with AllowDeletes: %+v", plan2)
	}
}

// --- dependency ordering ---

func TestTopoOrderBadgeAfterAsset(t *testing.T) {
	badge := Resource{
		Kind: KindBadge, Name: "winner",
		DependsOn: []ResourceRef{{Kind: KindAsset, Name: "icon.png"}},
		Inputs:    BadgeInputs{Name: "Winner", Icon: "icon.png"},
	}
	asset := Resource{Kind: KindAsset, Name: "icon.png", Inputs: AssetInputs{File: "icon.png"}}

	// Badge listed FIRST: topo sort must still put the asset first.
	plan, err := BuildPlan([]Resource{badge, asset}, NewState(), PlanOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if plan.Steps[0].Ref.Key() != "asset/icon.png" || plan.Steps[1].Ref.Key() != "badge/winner" {
		t.Fatalf("order = %v, %v", plan.Steps[0].Ref, plan.Steps[1].Ref)
	}
}

func TestTopoDeterministicWithinLevel(t *testing.T) {
	rs := []Resource{
		{Kind: KindPlaceFile, Name: "zeta", Inputs: PlaceFileInputs{PlaceID: 1}},
		{Kind: KindAsset, Name: "b.png", Inputs: AssetInputs{File: "b.png"}},
		{Kind: KindAsset, Name: "a.png", Inputs: AssetInputs{File: "a.png"}},
	}
	plan, err := BuildPlan(rs, NewState(), PlanOptions{})
	if err != nil {
		t.Fatal(err)
	}
	var keys []string
	for _, s := range plan.Steps {
		keys = append(keys, s.Ref.Key())
	}
	want := []string{"asset/a.png", "asset/b.png", "place_file/zeta"}
	if !reflect.DeepEqual(keys, want) {
		t.Fatalf("order = %v, want %v", keys, want)
	}
}

func TestTopoCycleError(t *testing.T) {
	a := Resource{Kind: KindBadge, Name: "a", DependsOn: []ResourceRef{{KindBadge, "b"}}, Inputs: BadgeInputs{}}
	b := Resource{Kind: KindBadge, Name: "b", DependsOn: []ResourceRef{{KindBadge, "a"}}, Inputs: BadgeInputs{}}
	if _, err := BuildPlan([]Resource{a, b}, NewState(), PlanOptions{}); err == nil {
		t.Fatal("cycle not detected")
	}
}

func TestTopoUnknownDependencyError(t *testing.T) {
	a := Resource{Kind: KindBadge, Name: "a", DependsOn: []ResourceRef{{KindAsset, "missing"}}, Inputs: BadgeInputs{}}
	if _, err := BuildPlan([]Resource{a}, NewState(), PlanOptions{}); err == nil {
		t.Fatal("unknown dependency not detected")
	}
}

// --- config -> resources ---

func testConfig(env config.Environment) *config.Config {
	return &config.Config{
		Assets: &config.AssetsConfig{Creator: config.Creator{Type: "group", ID: 99}},
		Deploy: &config.DeployConfig{Environments: map[string]config.Environment{"dev": env}},
	}
}

func writeFile(t *testing.T, dir, name string, data []byte) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestBuildResourcesGraph(t *testing.T) {
	dir := t.TempDir()
	writeFile(t, dir, "game.rbxl", []byte("rbxl-bytes"))
	writeFile(t, dir, "icon.png", []byte("png-bytes"))

	cfg := testConfig(config.Environment{
		UniverseID: 111,
		Places:     map[string]config.PlaceDeploy{"start": {File: "game.rbxl", PlaceID: 222}},
		Experience: &config.ExperienceConfig{Name: "My Game", Playability: "public"},
		Payments:   "free",
		Badges: map[string]config.Badge{
			"winner": {Name: "Winner!", Description: "won", Icon: "icon.png"},
		},
	})
	resources, universeID, err := BuildResources(dir, cfg, "dev")
	if err != nil {
		t.Fatal(err)
	}
	if universeID != 111 {
		t.Fatalf("universeID = %d", universeID)
	}
	byKey := map[string]Resource{}
	for _, r := range resources {
		byKey[r.Ref().Key()] = r
	}
	if len(byKey) != 4 {
		t.Fatalf("resources = %v", byKey)
	}
	pf := byKey["place_file/start"].Inputs.(PlaceFileInputs)
	if pf.PlaceID != 222 || pf.VersionType != "Published" || pf.FileHash == "" {
		t.Fatalf("place_file inputs = %+v", pf)
	}
	badge := byKey["badge/winner"]
	if len(badge.DependsOn) != 1 || badge.DependsOn[0].Key() != "asset/icon.png" {
		t.Fatalf("badge deps = %v", badge.DependsOn)
	}
	asset := byKey["asset/icon.png"].Inputs.(AssetInputs)
	if asset.AssetType != "Decal" || asset.CreatorType != "group" || asset.CreatorID != 99 || asset.FileHash == "" {
		t.Fatalf("asset inputs = %+v", asset)
	}
	exp := byKey["experience/universe"].Inputs.(ExperienceInputs)
	if exp.Name != "My Game" || exp.Playability != "public" || exp.Payments != "free" {
		t.Fatalf("experience inputs = %+v", exp)
	}

	// Unknown environment errors and names the available ones.
	if _, _, err := BuildResources(dir, cfg, "prod"); err == nil {
		t.Fatal("unknown environment not rejected")
	}
	// Missing place file errors.
	cfg2 := testConfig(config.Environment{
		UniverseID: 1,
		Places:     map[string]config.PlaceDeploy{"start": {File: "nope.rbxl", PlaceID: 2}},
	})
	if _, _, err := BuildResources(dir, cfg2, "dev"); err == nil {
		t.Fatal("missing place file not rejected")
	}
}

// TestBuildResourcesExpandedGraph covers the v1.1 kinds: game passes (with
// an icon shared with a badge deduping to ONE asset), experience icon,
// thumbnails, developer products, social links, place_config emission, and
// versionType wiring from the config.
func TestBuildResourcesExpandedGraph(t *testing.T) {
	dir := t.TempDir()
	writeFile(t, dir, "game.rbxl", []byte("rbxl-bytes"))
	writeFile(t, dir, "shared.png", []byte("shared-icon"))
	writeFile(t, dir, "icon.png", []byte("game-icon"))
	writeFile(t, dir, "thumb1.png", []byte("thumb-one"))
	writeFile(t, dir, "thumb2.png", []byte("thumb-two"))

	price := int64(150)
	cfg := testConfig(config.Environment{
		UniverseID: 111,
		Places: map[string]config.PlaceDeploy{
			"start": {
				File: "game.rbxl", PlaceID: 222,
				Name: "Start Place", Description: "the start", MaxPlayers: 30,
				VersionType: "saved",
			},
		},
		Experience: &config.ExperienceConfig{
			Name:           "My Game",
			PrivateServers: &config.PrivateServers{Price: &price},
		},
		Badges:     map[string]config.Badge{"winner": {Name: "Winner!", Icon: "shared.png"}},
		GamePasses: map[string]config.GamePass{"vip": {Name: "VIP", Price: &price, Icon: "shared.png"}},
		Icon:       "icon.png",
		Thumbnails: []string{"thumb1.png", "thumb2.png"},
		Products:   map[string]config.Product{"coins": {Name: "100 Coins", Price: 25}},
		SocialLinks: map[string]config.SocialLink{
			"discord": {Title: "Join us", URL: "https://discord.gg/x", Type: "discord"},
		},
	})
	resources, universeID, err := BuildResources(dir, cfg, "dev")
	if err != nil {
		t.Fatal(err)
	}
	if universeID != 111 {
		t.Fatalf("universeID = %d", universeID)
	}
	byKey := map[string]Resource{}
	for _, r := range resources {
		if _, dup := byKey[r.Ref().Key()]; dup {
			t.Fatalf("duplicate resource %s", r.Ref().Key())
		}
		byKey[r.Ref().Key()] = r
	}
	wantKeys := []string{
		"place_file/start", "place_config/start", "experience/universe",
		"asset/shared.png", "badge/winner", "game_pass/vip",
		"experience_icon/icon", "experience_thumbnails/thumbnails",
		"developer_product/coins", "social_link/discord",
	}
	if len(byKey) != len(wantKeys) {
		t.Fatalf("got %d resources, want %d: %v", len(byKey), len(wantKeys), byKey)
	}
	for _, k := range wantKeys {
		if _, ok := byKey[k]; !ok {
			t.Errorf("missing resource %s", k)
		}
	}

	// versionType from config ("saved" -> "Saved").
	pf := byKey["place_file/start"].Inputs.(PlaceFileInputs)
	if pf.VersionType != "Saved" {
		t.Errorf("versionType = %q, want Saved", pf.VersionType)
	}
	// place_config carries name/description/maxPlayers.
	pc := byKey["place_config/start"].Inputs.(PlaceConfigInputs)
	if pc.PlaceID != 222 || pc.Name != "Start Place" || pc.Description != "the start" || pc.ServerSize != 30 {
		t.Errorf("place_config inputs = %+v", pc)
	}
	// privateServers price flows into experience inputs.
	exp := byKey["experience/universe"].Inputs.(ExperienceInputs)
	if exp.PrivateServerPrice == nil || *exp.PrivateServerPrice != 150 {
		t.Errorf("experience inputs = %+v", exp)
	}
	// The shared icon deduped to one asset; both badge and pass depend on it.
	badge := byKey["badge/winner"]
	pass := byKey["game_pass/vip"]
	if len(badge.DependsOn) != 1 || badge.DependsOn[0].Key() != "asset/shared.png" {
		t.Errorf("badge deps = %v", badge.DependsOn)
	}
	if len(pass.DependsOn) != 1 || pass.DependsOn[0].Key() != "asset/shared.png" {
		t.Errorf("pass deps = %v", pass.DependsOn)
	}
	gp := pass.Inputs.(GamePassInputs)
	if gp.Name != "VIP" || gp.Price == nil || *gp.Price != 150 || gp.Icon != "shared.png" {
		t.Errorf("game pass inputs = %+v", gp)
	}
	// Thumbnails: ordered files + matching hashes.
	th := byKey["experience_thumbnails/thumbnails"].Inputs.(ExperienceThumbnailsInputs)
	if !reflect.DeepEqual(th.Files, []string{"thumb1.png", "thumb2.png"}) || len(th.FileHashes) != 2 {
		t.Errorf("thumbnail inputs = %+v", th)
	}
	if th.FileHashes[0] == th.FileHashes[1] {
		t.Error("distinct thumbnail files hashed identically")
	}
	ic := byKey["experience_icon/icon"].Inputs.(ExperienceIconInputs)
	if ic.File != "icon.png" || ic.FileHash == "" {
		t.Errorf("icon inputs = %+v", ic)
	}
	dp := byKey["developer_product/coins"].Inputs.(DeveloperProductInputs)
	if dp.Name != "100 Coins" || dp.Price != 25 {
		t.Errorf("product inputs = %+v", dp)
	}
	sl := byKey["social_link/discord"].Inputs.(SocialLinkInputs)
	if sl.Title != "Join us" || sl.URL != "https://discord.gg/x" || sl.Type != "discord" {
		t.Errorf("social link inputs = %+v", sl)
	}

	// Missing icon / thumbnail files error clearly at build time.
	for _, env := range []config.Environment{
		{UniverseID: 1, Icon: "missing.png"},
		{UniverseID: 1, Thumbnails: []string{"missing.png"}},
		{UniverseID: 1, GamePasses: map[string]config.GamePass{"vip": {Name: "VIP", Icon: "missing.png"}}},
	} {
		if _, _, err := BuildResources(dir, testConfig(env), "dev"); err == nil {
			t.Errorf("missing file not rejected for env %+v", env)
		}
	}
}

// TestPlanDiffsNewKinds: each v1.1 kind plans create when absent, no-op when
// the recorded hash matches, and update when any input changes.
func TestPlanDiffsNewKinds(t *testing.T) {
	price := int64(99)
	cases := []struct {
		res     Resource
		changed any // same kind, one input changed
	}{
		{
			Resource{Kind: KindGamePass, Name: "vip", Inputs: GamePassInputs{Name: "VIP", Price: &price}},
			GamePassInputs{Name: "VIP"}, // price removed -> off sale
		},
		{
			Resource{Kind: KindExperienceIcon, Name: "icon", Inputs: ExperienceIconInputs{File: "icon.png", FileHash: "sha256:a"}},
			ExperienceIconInputs{File: "icon.png", FileHash: "sha256:b"},
		},
		{
			Resource{Kind: KindExperienceThumbnails, Name: "thumbnails", Inputs: ExperienceThumbnailsInputs{Files: []string{"a.png", "b.png"}, FileHashes: []string{"sha256:a", "sha256:b"}}},
			// Reorder only: same files, same hashes, different order.
			ExperienceThumbnailsInputs{Files: []string{"b.png", "a.png"}, FileHashes: []string{"sha256:b", "sha256:a"}},
		},
		{
			Resource{Kind: KindDeveloperProduct, Name: "coins", Inputs: DeveloperProductInputs{Name: "Coins", Price: 10}},
			DeveloperProductInputs{Name: "Coins", Price: 20},
		},
		{
			Resource{Kind: KindSocialLink, Name: "discord", Inputs: SocialLinkInputs{Title: "Join", URL: "https://discord.gg/x", Type: "discord"}},
			SocialLinkInputs{Title: "Join!", URL: "https://discord.gg/x", Type: "discord"},
		},
	}
	for _, tc := range cases {
		key := tc.res.Ref().Key()

		// Absent from state -> create.
		plan, err := BuildPlan([]Resource{tc.res}, NewState(), PlanOptions{})
		if err != nil {
			t.Fatal(err)
		}
		if plan.Steps[0].Op != OpCreate {
			t.Errorf("%s: op = %v, want create", key, plan.Steps[0].Op)
		}

		// Matching hash -> no-op.
		hash, err := HashInputs(tc.res.Inputs)
		if err != nil {
			t.Fatal(err)
		}
		st := NewState()
		st.Resources[key] = &StateEntry{InputsHash: hash}
		plan, err = BuildPlan([]Resource{tc.res}, st, PlanOptions{})
		if err != nil {
			t.Fatal(err)
		}
		if plan.Steps[0].Op != OpNoop {
			t.Errorf("%s: op = %v, want no-op", key, plan.Steps[0].Op)
		}

		// Changed inputs -> update.
		changed := tc.res
		changed.Inputs = tc.changed
		plan, err = BuildPlan([]Resource{changed}, st, PlanOptions{})
		if err != nil {
			t.Fatal(err)
		}
		if plan.Steps[0].Op != OpUpdate {
			t.Errorf("%s: op = %v, want update after input change", key, plan.Steps[0].Op)
		}
	}
}
