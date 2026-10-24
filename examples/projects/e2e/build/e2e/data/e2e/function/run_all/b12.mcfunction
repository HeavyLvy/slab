execute unless score #run_all.v_k_2 e2e <= #run_all.v_end_2 e2e run return 0
scoreboard players operation #run_all.v_p e2e *= #run_all.v_k_2 e2e
scoreboard players add #run_all.v_k_2 e2e 1
return run function e2e:run_all/b12
