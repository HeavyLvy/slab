scoreboard players operation #mathx__lt.v_a e2e = #run_all.v_c e2e
scoreboard players set #mathx__lt.v_b e2e 5
function e2e:mathx__lt
scoreboard players operation #run_all.t_0 e2e = #ret e2e
execute unless score #run_all.t_0 e2e matches 1 run return 0
scoreboard players add #run_all.v_c e2e 1
return run function e2e:run_all/b21
