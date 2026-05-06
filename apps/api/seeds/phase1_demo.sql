INSERT INTO events (
    id,
    title,
    kind,
    drug_name,
    sponsor,
    indication,
    decision_date,
    status,
    outcome,
    resolved_at
)
VALUES
    ('10000000-0000-4000-8000-000000000001', 'Athenza PDUFA Jan 2025', 'fda_pdufa', 'Athenza', 'Helix Pharma', 'Chronic migraine prevention', '2025-01-10', 'resolved', 'approved', now()),
    ('10000000-0000-4000-8000-000000000002', 'Borelex PDUFA Feb 2025', 'fda_pdufa', 'Borelex', 'Northstar Bio', 'Moderate ulcerative colitis', '2025-02-18', 'resolved', 'rejected', now()),
    ('10000000-0000-4000-8000-000000000003', 'Clymera PDUFA Mar 2025', 'fda_pdufa', 'Clymera', 'Arcturus Therapeutics', 'Second-line NSCLC', '2025-03-12', 'resolved', 'approved', now()),
    ('10000000-0000-4000-8000-000000000004', 'Draxifen PDUFA Apr 2025', 'fda_pdufa', 'Draxifen', 'Blue Harbor Labs', 'Type 2 diabetes adjunct', '2025-04-03', 'resolved', 'rejected', now()),
    ('10000000-0000-4000-8000-000000000005', 'Elfimed PDUFA May 2025', 'fda_pdufa', 'Elfimed', 'Beacon Clinical', 'Relapsed AML', '2025-05-28', 'resolved', 'approved', now()),
    ('10000000-0000-4000-8000-000000000006', 'Feronil PDUFA Jun 2025', 'fda_pdufa', 'Feronil', 'Summit Pharma', 'Atopic dermatitis severe', '2025-06-20', 'resolved', 'approved', now()),
    ('10000000-0000-4000-8000-000000000007', 'Glycora PDUFA Jul 2025', 'fda_pdufa', 'Glycora', 'Riverbend Therapeutics', 'Heart failure with preserved EF', '2025-07-15', 'resolved', 'rejected', now()),
    ('10000000-0000-4000-8000-000000000008', 'Hylotan PDUFA Aug 2025', 'fda_pdufa', 'Hylotan', 'Ivy Biomed', 'Generalized myasthenia gravis', '2025-08-26', 'resolved', 'approved', now()),
    ('10000000-0000-4000-8000-000000000009', 'Ionvera PDUFA Nov 2026', 'fda_pdufa', 'Ionvera', 'Mosaic Bio', 'Primary biliary cholangitis', '2026-11-18', 'upcoming', NULL, NULL),
    ('10000000-0000-4000-8000-000000000010', 'Jorexan PDUFA Dec 2026', 'fda_pdufa', 'Jorexan', 'Pinnacle Medicines', 'Adult focal epilepsy', '2026-12-09', 'upcoming', NULL, NULL)
ON CONFLICT (id) DO NOTHING;

INSERT INTO forecasts (id, user_id, event_id, probability, rationale)
VALUES
    ('20000000-0000-4000-8000-000000000001', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000001', 0.7300, 'Strong endpoint signal and clean safety profile.'),
    ('20000000-0000-4000-8000-000000000002', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000002', 0.6100, 'Regulatory questions appeared manageable.'),
    ('20000000-0000-4000-8000-000000000003', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000003', 0.5500, 'Mixed readout but favorable subgroup performance.'),
    ('20000000-0000-4000-8000-000000000004', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000004', 0.4200, 'Adcom briefing raised efficacy durability concerns.'),
    ('20000000-0000-4000-8000-000000000005', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000005', 0.6900, 'Prior-line evidence de-risked this indication.'),
    ('20000000-0000-4000-8000-000000000006', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000006', 0.6400, 'Strong phase 3 consistency across endpoints.'),
    ('20000000-0000-4000-8000-000000000007', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000007', 0.5800, 'Cardio safety uncertainty might still pass panel.'),
    ('20000000-0000-4000-8000-000000000008', '00000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000008', 0.7700, 'Mechanism and prior evidence support high approval odds.')
ON CONFLICT (id) DO NOTHING;
