import oracle, sys
for i in sys.argv[1:]:
    oracle.filter_mutants(i, [f"/tmp/cohort-oracle/{i}/agent{k}/repo/tests" for k in (0,1)])
